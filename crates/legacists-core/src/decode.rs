use crate::Encoding;
use encoding_rs::SHIFT_JIS;

pub struct Decoded {
    pub text: String,
    pub encoding: String,
    pub reason: String,
    pub ambiguous: bool,
}

pub fn decode(bytes: &[u8], manual: Option<&Encoding>) -> Option<Decoded> {
    if bytes.starts_with(&[0xff, 0xfe])
        || bytes.starts_with(&[0xfe, 0xff])
        || bytes.starts_with(&[0, 0, 0xfe, 0xff])
    {
        return None;
    }
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let content = if bom { &bytes[3..] } else { bytes };
    let utf8 = std::str::from_utf8(content).ok();
    let sjis = SHIFT_JIS.decode_without_bom_handling_and_without_replacement(content);
    let (text, encoding, reason, ambiguous) = match manual {
        Some(Encoding::Utf8) => (utf8?.to_owned(), "UTF-8", "手動指定", false),
        Some(Encoding::Sjis) if bom => return None,
        Some(Encoding::Sjis) => (
            sjis?.into_owned(),
            "SJIS系（CP932を含む）",
            "手動指定",
            false,
        ),
        None if bom => (utf8?.to_owned(), "UTF-8", "UTF-8 BOM", false),
        None => match (utf8, sjis) {
            (Some(u), Some(s)) if u == s => (u.to_owned(), "UTF-8", "共通文字／UTF-8扱い", false),
            (Some(u), Some(_)) => (u.to_owned(), "UTF-8", "判定が曖昧／UTF-8仮選択", true),
            (Some(u), None) => (u.to_owned(), "UTF-8", "UTF-8のみ復号成功", false),
            (None, Some(s)) => (
                s.into_owned(),
                "SJIS系（CP932を含む）",
                "SJIS系のみ復号成功",
                false,
            ),
            _ => return None,
        },
    };
    Some(Decoded {
        text,
        encoding: encoding.into(),
        reason: reason.into(),
        ambiguous,
    })
}
