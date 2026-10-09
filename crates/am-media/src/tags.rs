use crate::bmff::{Atom, FourCC};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CoverFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cover {
    pub format: CoverFormat,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub composer: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub copyright: Option<String>,
    pub lyrics: Option<String>,
    pub sort_title: Option<String>,
    pub sort_artist: Option<String>,
    pub sort_album: Option<String>,
    pub sort_composer: Option<String>,
    pub track_number: Option<u16>,
    pub track_total: Option<u16>,
    pub disc_number: Option<u16>,
    pub disc_total: Option<u16>,
    pub media_kind: Option<u8>,
    pub rating: Option<u8>,
    pub compilation: Option<bool>,
    pub gapless: Option<bool>,
    pub isrc: Option<String>,
    pub itunes_track_id: Option<u64>,
    pub itunes_artist_id: Option<u64>,
    pub itunes_playlist_id: Option<u64>,
    pub storefront_id: Option<u64>,
    pub xid: Option<String>,
    /// Declared in the tags JSON ("coverFormat": "jpeg"|"png"); the cover bytes
    /// themselves arrive through a separate pointer in the wasm export.
    pub cover_format: Option<CoverFormat>,
    #[serde(skip)]
    pub cover: Option<Cover>,
}

const DATA_TYPE_BINARY: u32 = 0;
const DATA_TYPE_UTF8: u32 = 1;
const DATA_TYPE_COVER_JPEG: u32 = 13;
const DATA_TYPE_COVER_PNG: u32 = 14;
const DATA_TYPE_I8: u32 = 21;

const KEY_ALBUM: FourCC = *b"\xa9alb";
const KEY_ALBUM_ARTIST: FourCC = *b"aART";
const KEY_PLAYLIST_ID: FourCC = *b"plID";
const KEY_SORT_ALBUM: FourCC = *b"soal";
const KEY_ARTIST: FourCC = *b"\xa9ART";
const KEY_ARTIST_ID: FourCC = *b"atID";
const KEY_SORT_ARTIST: FourCC = *b"soar";
const KEY_COMPOSER: FourCC = *b"\xa9wrt";
const KEY_SORT_COMPOSER: FourCC = *b"soco";
const KEY_COPYRIGHT: FourCC = *b"cprt";
const KEY_RELEASE_DATE: FourCC = *b"\xa9day";
const KEY_DISC: FourCC = *b"disk";
const KEY_GAPLESS: FourCC = *b"pgap";
const KEY_COMPILATION: FourCC = *b"cpil";
const KEY_GENRE: FourCC = *b"\xa9gen";
const KEY_LYRICS: FourCC = *b"\xa9lyr";
const KEY_MEDIA_KIND: FourCC = *b"stik";
const KEY_RATING: FourCC = *b"rtng";
const KEY_STOREFRONT_ID: FourCC = *b"sfID";
const KEY_TITLE: FourCC = *b"\xa9nam";
const KEY_TRACK_ID: FourCC = *b"cnID";
const KEY_SORT_TITLE: FourCC = *b"sonm";
const KEY_TRACK: FourCC = *b"trkn";
const KEY_XID: FourCC = *b"xid ";
const KEY_COVER: FourCC = *b"covr";
const KEY_FREEFORM: FourCC = *b"----";

const FREEFORM_MEAN: FourCC = *b"mean";
const FREEFORM_NAME: FourCC = *b"name";

fn data_box(data_type: u32, value: &[u8]) -> Atom {
    let mut payload = Vec::with_capacity(8 + value.len());
    payload.extend_from_slice(&data_type.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(value);
    Atom::data(b"data", payload)
}

fn full_box(kind: &FourCC, value: &[u8]) -> Atom {
    let mut payload = Vec::with_capacity(4 + value.len());
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(value);
    Atom::data(kind, payload)
}

fn item(key: &FourCC, datas: Vec<Atom>) -> Atom {
    let mut body = Vec::new();
    for data in &datas {
        data.encode(&mut body);
    }
    Atom::data(key, body)
}

fn text_item(key: &FourCC, value: &str) -> Option<Atom> {
    if value.trim().is_empty() {
        return None;
    }
    Some(item(key, vec![data_box(DATA_TYPE_UTF8, value.as_bytes())]))
}

fn text_item_owned(key: &FourCC, value: &Option<String>) -> Option<Atom> {
    value.as_deref().and_then(|v| text_item(key, v))
}

fn int8_item(key: &FourCC, value: u8) -> Atom {
    item(key, vec![data_box(DATA_TYPE_I8, &[value])])
}

fn bool_item(key: &FourCC, value: bool) -> Atom {
    int8_item(key, if value { 1 } else { 0 })
}

fn u64_item(key: &FourCC, value: u64) -> Atom {
    item(key, vec![data_box(DATA_TYPE_BINARY, &value.to_be_bytes())])
}

fn pair_item(key: &FourCC, number: Option<u16>, total: Option<u16>) -> Option<Atom> {
    let n = number.unwrap_or(0);
    let t = total.unwrap_or(0);
    if n == 0 && t == 0 {
        return None;
    }
    let mut value = [0u8; 8];
    value[2..4].copy_from_slice(&n.to_be_bytes());
    value[6..8].copy_from_slice(&t.to_be_bytes());
    Some(item(key, vec![data_box(DATA_TYPE_BINARY, &value)]))
}

fn freeform_item(mean: &str, name: &str, value: &str) -> Option<Atom> {
    if value.trim().is_empty() {
        return None;
    }
    let mean_atom = full_box(&FREEFORM_MEAN, mean.as_bytes());
    let name_atom = full_box(&FREEFORM_NAME, name.as_bytes());
    let data_atom = data_box(DATA_TYPE_UTF8, value.as_bytes());
    Some(item(&KEY_FREEFORM, vec![mean_atom, name_atom, data_atom]))
}

impl Tags {
    pub fn to_ilst(&self) -> Vec<Atom> {
        let mut atoms: Vec<Atom> = Vec::new();

        if let Some(atom) = text_item_owned(&KEY_ALBUM, &self.album) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_ALBUM_ARTIST, &self.album_artist) {
            atoms.push(atom);
        }
        if let Some(id) = self.itunes_playlist_id {
            atoms.push(u64_item(&KEY_PLAYLIST_ID, id));
        }
        if let Some(atom) = text_item_owned(&KEY_SORT_ALBUM, &self.sort_album) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_ARTIST, &self.artist) {
            atoms.push(atom);
        }
        if let Some(id) = self.itunes_artist_id {
            atoms.push(u64_item(&KEY_ARTIST_ID, id));
        }
        if let Some(atom) = text_item_owned(&KEY_SORT_ARTIST, &self.sort_artist) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_COMPOSER, &self.composer) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_SORT_COMPOSER, &self.sort_composer) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_COPYRIGHT, &self.copyright) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_RELEASE_DATE, &self.release_date) {
            atoms.push(atom);
        }
        if let Some(atom) = pair_item(&KEY_DISC, self.disc_number, self.disc_total) {
            atoms.push(atom);
        }
        if let Some(gapless) = self.gapless {
            atoms.push(bool_item(&KEY_GAPLESS, gapless));
        }
        if let Some(compilation) = self.compilation {
            atoms.push(bool_item(&KEY_COMPILATION, compilation));
        }
        if let Some(atom) = text_item_owned(&KEY_GENRE, &self.genre) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_LYRICS, &self.lyrics) {
            atoms.push(atom);
        }
        if let Some(kind) = self.media_kind {
            atoms.push(int8_item(&KEY_MEDIA_KIND, kind));
        }
        if let Some(rating) = self.rating {
            atoms.push(int8_item(&KEY_RATING, rating));
        }
        if let Some(id) = self.storefront_id {
            atoms.push(u64_item(&KEY_STOREFRONT_ID, id));
        }
        if let Some(atom) = text_item_owned(&KEY_TITLE, &self.title) {
            atoms.push(atom);
        }
        if let Some(id) = self.itunes_track_id {
            atoms.push(u64_item(&KEY_TRACK_ID, id));
        }
        if let Some(atom) = text_item_owned(&KEY_SORT_TITLE, &self.sort_title) {
            atoms.push(atom);
        }
        if let Some(atom) = pair_item(&KEY_TRACK, self.track_number, self.track_total) {
            atoms.push(atom);
        }
        if let Some(atom) = text_item_owned(&KEY_XID, &self.xid) {
            atoms.push(atom);
        }
        if let Some(cover) = &self.cover {
            let data_type = match cover.format {
                CoverFormat::Jpeg => DATA_TYPE_COVER_JPEG,
                CoverFormat::Png => DATA_TYPE_COVER_PNG,
            };
            atoms.push(item(&KEY_COVER, vec![data_box(data_type, &cover.data)]));
        }
        if let Some(atom) = freeform_item("com.apple.iTunes", "ISRC", self.isrc.as_deref().unwrap_or("")) {
            atoms.push(atom);
        }

        atoms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bmff::{Atom, Body};

    fn read_data_box(payload: &[u8]) -> (u32, u32, &[u8]) {
        assert!(payload.len() >= 8);
        let data_type = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let locale = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        (data_type, locale, &payload[8..])
    }

    fn child_payloads(atom: &Atom) -> Vec<(FourCC, Vec<u8>)> {
        parse_children(atom)
    }

    #[test]
    fn text_atom_structure() {
        let tags = Tags {
            title: Some("Hello".into()),
            ..Default::default()
        };
        let atoms = tags.to_ilst();
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].kind, KEY_TITLE);
        let children = child_payloads(&atoms[0]);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, *b"data");
        let (dt, locale, value) = read_data_box(&children[0].1);
        assert_eq!(dt, 1);
        assert_eq!(locale, 0);
        assert_eq!(value, b"Hello");
    }

    #[test]
    fn trkn_encoding() {
        let tags = Tags {
            track_number: Some(3),
            track_total: Some(12),
            ..Default::default()
        };
        let atoms = tags.to_ilst();
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].kind, KEY_TRACK);
        let children = child_payloads(&atoms[0]);
        let (dt, locale, value) = read_data_box(&children[0].1);
        assert_eq!(dt, 0);
        assert_eq!(locale, 0);
        assert_eq!(value, &[0, 0, 0, 3, 0, 0, 0, 12]);
    }

    #[test]
    fn empty_tags_empty_vec() {
        let tags = Tags::default();
        assert!(tags.to_ilst().is_empty());
    }

    #[test]
    fn bool_atoms_cpil_pgap() {
        for (field, key) in [(true, KEY_COMPILATION), (false, KEY_COMPILATION)] {
            let tags = Tags { compilation: Some(field), ..Default::default() };
            let atoms = tags.to_ilst();
            assert_eq!(atoms.len(), 1);
            assert_eq!(atoms[0].kind, key);
            let children = child_payloads(&atoms[0]);
            let (dt, _, value) = read_data_box(&children[0].1);
            assert_eq!(dt, 21);
            assert_eq!(value, &[if field { 1 } else { 0 }]);
        }
        let tags = Tags { gapless: Some(true), ..Default::default() };
        let atoms = tags.to_ilst();
        assert_eq!(atoms[0].kind, KEY_GAPLESS);
    }

    #[test]
    fn deserializes_tags_js_json() {
        // Representative output of src/ui/tags.js buildSongTags().
        let json = r#"{"title":"Song Title","artist":"Artist","album":"Album","composer":"Composer","genre":"Pop","releaseDate":"2024-05-01","copyright":"℗ 2024 Label","trackNumber":3,"trackTotal":12,"discNumber":1,"discTotal":2,"mediaKind":1,"rating":4,"lyrics":"la la\nla","isrc":"US1234567890","coverFormat":"jpeg","itunesTrackId":1234567890,"itunesPlaylistId":987654321}"#;
        let tags: Tags = serde_json::from_str(json).unwrap();
        assert_eq!(tags.title.as_deref(), Some("Song Title"));
        assert_eq!(tags.track_number, Some(3));
        assert_eq!(tags.track_total, Some(12));
        assert_eq!(tags.media_kind, Some(1));
        assert_eq!(tags.rating, Some(4));
        assert_eq!(tags.cover_format, Some(CoverFormat::Jpeg));
        assert_eq!(tags.itunes_track_id, Some(1234567890));
        assert_eq!(tags.itunes_playlist_id, Some(987654321));
        assert_eq!(tags.cover, None);
        // Unknown fields are ignored; missing fields default to None.
        let tags: Tags = serde_json::from_str(r#"{"title":"x","bogus":1}"#).unwrap();
        assert_eq!(tags.title.as_deref(), Some("x"));
        assert_eq!(tags.artist, None);
    }

    #[test]
    fn xid_key_has_trailing_space() {
        let tags = Tags {
            xid: Some("abc".into()),
            ..Default::default()
        };
        let atoms = tags.to_ilst();
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].kind, *b"xid ");
    }

    #[test]
    fn isrc_freeform_structure() {
        let tags = Tags {
            isrc: Some("US1234567890".into()),
            ..Default::default()
        };
        let atoms = tags.to_ilst();
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].kind, KEY_FREEFORM);
        let children = child_payloads(&atoms[0]);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].0, *b"mean");
        assert_eq!(children[1].0, *b"name");
        assert_eq!(children[2].0, *b"data");

        let mut expected_mean = vec![0, 0, 0, 0];
        expected_mean.extend_from_slice(b"com.apple.iTunes");
        assert_eq!(children[0].1, expected_mean);

        let mut expected_name = vec![0, 0, 0, 0];
        expected_name.extend_from_slice(b"ISRC");
        assert_eq!(children[1].1, expected_name);

        let (dt, locale, value) = read_data_box(&children[2].1);
        assert_eq!(dt, 1);
        assert_eq!(locale, 0);
        assert_eq!(value, b"US1234567890");
    }

    fn parse_children(atom: &Atom) -> Vec<(FourCC, Vec<u8>)> {
        match &atom.body {
            Body::Data(buf) => {
                let mut out = Vec::new();
                let mut pos = 0usize;
                while pos + 8 <= buf.len() {
                    let size = u32::from_be_bytes([
                        buf[pos],
                        buf[pos + 1],
                        buf[pos + 2],
                        buf[pos + 3],
                    ]) as usize;
                    if size < 8 || pos + size > buf.len() {
                        break;
                    }
                    let mut kind = [0u8; 4];
                    kind.copy_from_slice(&buf[pos + 4..pos + 8]);
                    let payload = buf[pos + 8..pos + size].to_vec();
                    out.push((kind, payload));
                    pos += size;
                }
                out
            }
            Body::Tree { children, .. } => children
                .iter()
                .map(|c| (c.kind, c.bytes().to_vec()))
                .collect(),
        }
    }
}
