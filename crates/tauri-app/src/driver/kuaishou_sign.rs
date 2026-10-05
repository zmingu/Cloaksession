//! 快手 App 接口 NS 签名算法（`sig` 计算）。
//!
//! 逐行移植自 jieger 参考实现
//! `F:\jieger\electron\main\services\kuaishou\sign.ts`（Electron/TypeScript），
//! 保持完全一致的语义，便于与真机抓包/Node 实现交叉验证。
//!
//! # 算法概要
//!
//! 1. 收集参与签名的参数（原始 `key=value` 串），剔除 `includeInSignature === false`
//!    的参数，再剔除 `key == "sig"` 的参数。
//! 2. 对参数列表做**字符串排序**（JS `Array.prototype.sort()` 默认按 UTF-16 码元升序）
//!    后 `join("")` 得到签名基串。
//! 3. 对基串执行 [`percent_decode_for_kuaishou_sig`]：`+` 先替换为 `%2B`，
//!    再把 `%XX` 解成单个字节，其余字符按其 UTF-8 字节原样推入。
//! 4. `md5(基串解码字节 + salt 的 ASCII 字节)`，输出小写 hex。
//!
//! salt 由 `client_key` 查表得到（见 [`KUAISHOU_CLIENT_KEY_SIG_SALTS`]），
//! 未命中时使用 [`KUAISHOU_DEFAULT_SIG_SALT`]。
//!
//! 本模块是**纯计算工具**，不发起任何网络请求。
//!
//! 目前尚无 crate 内调用方（后续节点的快手 App 接口请求才会消费它），
//! 故整体 `allow(dead_code)`，与 `driver/sub_account.rs` 的既有做法一致。
#![allow(dead_code)]

use md5::{Digest, Md5};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

/// 默认签名 salt（无匹配 `client_key` 时使用）。
pub const KUAISHOU_DEFAULT_SIG_SALT: &str = "382700b563f4";

/// `client_key` → salt 映射表（与参考实现逐项一致）。
pub const KUAISHOU_CLIENT_KEY_SIG_SALTS: &[(&str, &str)] = &[
    ("3c2cd3f3", "382700b563f4"),
    ("74901a18", "0138f308797"),
    ("2ac2a76d", "772867c19925"),
    ("63b2bdd7", "c6e65c1f66a2"),
    ("56c3713c", "23caab00356c"),
    ("8d219c8d", "ca8e86efb32e"),
];

/// 参与签名的参数：原始 `key=value` 串 + 是否计入签名。
///
/// 对应参考实现的 `KuaishouSigParam`。普通字符串参数默认计入签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KuaishouSigParam {
    /// 原始参数串，形如 `key=value`（或仅 `key`）。
    pub raw: String,
    /// `false` 时该参数不参与签名（例如 multipart 的文件字段）。
    pub include_in_signature: bool,
}

impl KuaishouSigParam {
    /// 计入签名的参数。
    pub fn new(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            include_in_signature: true,
        }
    }

    /// 不计入签名的参数（`includeInSignature === false`）。
    pub fn excluded(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            include_in_signature: false,
        }
    }
}

impl From<&str> for KuaishouSigParam {
    fn from(value: &str) -> Self {
        KuaishouSigParam::new(value)
    }
}

impl From<&String> for KuaishouSigParam {
    fn from(value: &String) -> Self {
        KuaishouSigParam::new(value.clone())
    }
}

impl From<String> for KuaishouSigParam {
    fn from(value: String) -> Self {
        KuaishouSigParam::new(value)
    }
}

/// encodeURIComponent 未转义的字符集：`A-Za-z0-9-_.!~*'()`。
///
/// `percent-encoding` 只对落在 `AsciiSet` 中的 ASCII 字节做百分号编码
/// （非 ASCII 字节始终编码），因此把除未转义集以外的所有 ASCII 字符加入集合，
/// 即等价于 `encodeURIComponent` + 把 `!'()*` 替换为 `%XX`（大写）。
const ENCODE_COMPONENT_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// `encodeURIComponent(value)` 再把 `!'()*` 替换为 `%XX`（大写）。
///
/// 对应参考实现的 `encodeComponent`。
pub fn encode_component(value: &str) -> String {
    utf8_percent_encode(value, ENCODE_COMPONENT_SET).to_string()
}

/// 取参数串的 key（第一个 `=` 之前的部分；无 `=` 时返回整串）。
fn get_param_key(raw: &str) -> &str {
    match raw.find('=') {
        Some(idx) => &raw[..idx],
        None => raw,
    }
}

/// 从参数列表中推断 `client_key`（第一个 `client_key=` 前缀后的值）。
fn infer_client_key(params: &[String]) -> Option<String> {
    for raw in params {
        if let Some(rest) = raw.strip_prefix("client_key=") {
            return Some(rest.to_string());
        }
    }
    None
}

/// 过滤参数：剔除 `include_in_signature == false` 的项，再剔除 `key == "sig"` 的项。
///
/// 对应参考实现的 `collectKuaishouSigParams`。
pub fn collect_kuaishou_sig_params(params: &[KuaishouSigParam]) -> Vec<String> {
    params
        .iter()
        .filter(|p| p.include_in_signature)
        .map(|p| p.raw.clone())
        .filter(|raw| get_param_key(raw) != "sig")
        .collect()
}

/// 把 `&[String]` 参数视为全部计入签名，转换为 `KuaishouSigParam` 列表。
fn to_params(params: &[String]) -> Vec<KuaishouSigParam> {
    params.iter().cloned().map(KuaishouSigParam::new).collect()
}

/// 解析 salt：显式 `salt` 优先；否则按 `client_key` 查表；否则默认 salt。
///
/// 对应参考实现的 `resolveKuaishouSigSalt`。空字符串视为未提供
/// （与 JS 的 falsy 判断一致）。
pub fn resolve_kuaishou_sig_salt(
    params: &[String],
    client_key: Option<&str>,
    salt: Option<&str>,
) -> String {
    if let Some(s) = salt {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    let collected = collect_kuaishou_sig_params(&to_params(params));
    let effective = match client_key {
        Some(ck) => Some(ck.to_string()),
        None => infer_client_key(&collected),
    };
    if let Some(ck) = effective {
        if let Some((_, mapped)) = KUAISHOU_CLIENT_KEY_SIG_SALTS
            .iter()
            .find(|(key, _)| *key == ck.as_str())
        {
            return (*mapped).to_string();
        }
    }
    KUAISHOU_DEFAULT_SIG_SALT.to_string()
}

/// 计算签名基串：过滤后的参数按字符串升序排序后拼接。
///
/// 对应参考实现的 `createKuaishouSigBase`。
pub fn create_kuaishou_sig_base(params: &[String]) -> String {
    let mut collected = collect_kuaishou_sig_params(&to_params(params));
    collected.sort();
    collected.join("")
}

/// 计算签名基串（支持 `include_in_signature == false` 的参数）。
pub fn create_kuaishou_sig_base_from_params(params: &[KuaishouSigParam]) -> String {
    let mut collected = collect_kuaishou_sig_params(params);
    collected.sort();
    collected.join("")
}

/// `+` → `%2B`，然后解 `%XX` 为单字节；其余字符按 UTF-8 字节原样输出。
///
/// 对应参考实现的 `percentDecodeForKuaishouSig`（返回 `Buffer`，此处返回 `Vec<u8>`）。
pub fn percent_decode_for_kuaishou_sig(input: &str) -> Vec<u8> {
    let protected = input.replace('+', "%2B");
    let bytes = protected.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // 与参考实现一致：需要 `%` 之后至少还有 2 个字符（严格小于）。
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(value) = js_parse_int_hex(&bytes[i + 1..i + 3]) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        // 非转义序列：推入该字符完整的 UTF-8 字节，前进一个字符。
        if let Some(ch) = protected[i..].chars().next() {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

/// 模拟 JS `Number.parseInt(hex, 16)` 的宽松解析语义：
/// 允许前导空白与正负号，取尽可能长的十六进制前缀；无有效数字返回 `None`（NaN）。
///
/// 结果按 `u8` 截断（等价于 `Buffer.from([value])` 对越界/负值的环绕）。
fn js_parse_int_hex(bytes: &[u8]) -> Option<u8> {
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut negative = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        negative = bytes[i] == b'-';
        i += 1;
    }
    let digits_start = i;
    let mut value: i64 = 0;
    while i < bytes.len() {
        match (bytes[i] as char).to_digit(16) {
            Some(digit) => {
                value = value * 16 + digit as i64;
                i += 1;
            }
            None => break,
        }
    }
    if i == digits_start {
        return None;
    }
    if negative {
        value = -value;
    }
    Some(value as u8)
}

/// 字节切片 → 小写十六进制字符串。
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// 计算 `sig`：`md5(percentDecode(基串) + salt_ascii)` 的小写 hex。
///
/// 对应参考实现的 `signKuaishouRequest`。
pub fn sign_kuaishou_request(
    params: &[String],
    client_key: Option<&str>,
    salt: Option<&str>,
) -> String {
    let base = create_kuaishou_sig_base(params);
    let resolved_salt = resolve_kuaishou_sig_salt(params, client_key, salt);
    let mut hasher = Md5::new();
    hasher.update(percent_decode_for_kuaishou_sig(&base));
    // salt 以 ASCII 追加（salt 表全为 ASCII 十六进制）。
    hasher.update(resolved_salt.as_bytes());
    to_hex(&hasher.finalize())
}

/// 计算 `sig`（支持 `include_in_signature == false` 的参数）。
pub fn sign_kuaishou_request_with_params(
    params: &[KuaishouSigParam],
    client_key: Option<&str>,
    salt: Option<&str>,
) -> String {
    let base = create_kuaishou_sig_base_from_params(params);
    let collected = collect_kuaishou_sig_params(params);
    let resolved_salt = resolve_kuaishou_sig_salt(&collected, client_key, salt);
    let mut hasher = Md5::new();
    hasher.update(percent_decode_for_kuaishou_sig(&base));
    hasher.update(resolved_salt.as_bytes());
    to_hex(&hasher.finalize())
}

/// 拆分 query 字符串为参数列表（去掉前导 `?`，按 `&` 切分并丢弃空项）。
///
/// 对应参考实现的 `splitKuaishouQueryString`。
pub fn split_kuaishou_query_string(search: &str) -> Vec<String> {
    let trimmed = search.strip_prefix('?').unwrap_or(search);
    trimmed
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

/// 合并 baseUrl 已有 query 与新 query（均剔除 `sig`），与 body 一起签名，
/// 再追加 `sig=<hex>` 返回完整 URL。
///
/// 对应参考实现的 `buildSignedKuaishouUrl`。
pub fn build_signed_kuaishou_url(
    base_url: &str,
    query: &[String],
    body: &[String],
    client_key: Option<&str>,
) -> String {
    let (url_base, search) = match base_url.split_once('?') {
        Some((head, tail)) => (head, tail),
        None => (base_url, ""),
    };
    let existing_query = split_kuaishou_query_string(search);
    let mut merged: Vec<String> = existing_query
        .into_iter()
        .chain(query.iter().cloned())
        .filter(|raw| get_param_key(raw) != "sig")
        .collect();
    let mut signing_params = merged.clone();
    signing_params.extend(body.iter().cloned());
    let sig = sign_kuaishou_request(&signing_params, client_key, None);
    merged.push(format!("sig={sig}"));
    format!("{url_base}?{}", merged.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 金标准向量由 Node v24 直接运行参考实现
    /// `F:\jieger\electron\main\services\kuaishou\sign.ts` 生成（见交接文档）。
    #[test]
    fn golden_sorted_default_salt() {
        let params = vec!["b=2".to_string(), "a=1".to_string(), "c=3".to_string()];
        assert_eq!(create_kuaishou_sig_base(&params), "a=1b=2c=3");
        assert_eq!(
            sign_kuaishou_request(&params, None, None),
            "c22f3356d2a181232d71b3379294d1f8"
        );
    }

    #[test]
    fn golden_client_key_selects_salt() {
        let params = vec!["client_key=74901a18".to_string(), "a=1".to_string()];
        assert_eq!(
            resolve_kuaishou_sig_salt(&params, None, None),
            "0138f308797"
        );
        assert_eq!(
            sign_kuaishou_request(&params, None, None),
            "8feac093e542159e3e8f30be6f77eb42"
        );

        let params2 = vec!["client_key=2ac2a76d".to_string(), "foo=bar".to_string()];
        assert_eq!(
            sign_kuaishou_request(&params2, None, None),
            "617d3fb21c5df32c55a09dcade0947f6"
        );

        // 未知 client_key 回退默认 salt。
        let params3 = vec!["client_key=deadbeef".to_string(), "a=1".to_string()];
        assert_eq!(
            resolve_kuaishou_sig_salt(&params3, None, None),
            "382700b563f4"
        );
    }

    #[test]
    fn golden_explicit_salt() {
        let params = vec!["a=1".to_string()];
        assert_eq!(
            sign_kuaishou_request(&params, None, Some("customsalt")),
            "7f0288256919e9c0ab90a8a997ad958d"
        );
    }

    #[test]
    fn golden_sig_excluded_and_include_filter() {
        let params = vec![
            KuaishouSigParam::new("a=1"),
            KuaishouSigParam::new("sig=deadbeef"),
            KuaishouSigParam::excluded("b=2"),
            KuaishouSigParam::new("c=3"),
        ];
        assert_eq!(collect_kuaishou_sig_params(&params), vec!["a=1", "c=3"]);
        assert_eq!(create_kuaishou_sig_base_from_params(&params), "a=1c=3");
        assert_eq!(
            sign_kuaishou_request_with_params(&params, None, None),
            "25ed18f65d50a6653d7fe9c9a6561502"
        );
    }

    #[test]
    fn golden_percent_decode() {
        // `+` 保留为字面量加号（先替换为 %2B 再解码）。
        assert_eq!(
            percent_decode_for_kuaishou_sig("token=a+b****"),
            b"token=a+b****".to_vec()
        );
        assert_eq!(
            percent_decode_for_kuaishou_sig("a+b"),
            vec![0x61, 0x2b, 0x62]
        );
        // `%XX` 解为单字节（UTF-8 中文）。
        assert_eq!(
            percent_decode_for_kuaishou_sig("%E4%B8%AD"),
            vec![0xE4, 0xB8, 0xAD]
        );
        assert_eq!(
            String::from_utf8(percent_decode_for_kuaishou_sig("%E4%B8%AD")).unwrap(),
            "中"
        );
        // 混合：%41=%42 → 'A','B'，+ → '+'，%43 → 'C'。
        assert_eq!(
            String::from_utf8(percent_decode_for_kuaishou_sig("k=%41%42+%43")).unwrap(),
            "k=AB+C"
        );
    }

    #[test]
    fn golden_url_build_dedupes_sig() {
        let body = vec![
            "authorId=2617381505".to_string(),
            "needPhotoCount=true".to_string(),
            "needFansCount=true".to_string(),
            "country_code=cn".to_string(),
        ];
        let url = build_signed_kuaishou_url(
            "https://live.ksapisrv.com/rest/n/dynamicIcon/info?kpn=KUAISHOU&client_key=3c2cd3f3&sig=stale",
            &[],
            &body,
            None,
        );
        assert_eq!(
            url,
            "https://live.ksapisrv.com/rest/n/dynamicIcon/info?kpn=KUAISHOU&client_key=3c2cd3f3&sig=413653c40bbf9fe9b598214e09897dac"
        );
        // 旧 sig 被剔除，仅保留一个 sig。
        assert_eq!(url.matches("sig=").count(), 1);
    }

    #[test]
    fn golden_url_build_with_query() {
        let url = build_signed_kuaishou_url(
            "https://example.com/api",
            &["client_key=74901a18".to_string(), "b=2".to_string()],
            &["a=1".to_string()],
            None,
        );
        let expected =
            "https://example.com/api?client_key=74901a18&b=2&sig=2a47991a6123bdb1a50365277d0eec08";
        assert_eq!(url, expected);
    }

    #[test]
    fn query_split_and_collect() {
        assert_eq!(
            split_kuaishou_query_string("?a=1&b=2&"),
            vec!["a=1", "b=2"]
        );
        let params = vec![
            KuaishouSigParam::new("a=1"),
            KuaishouSigParam::excluded("b=2"),
            KuaishouSigParam::new("sig=x"),
            KuaishouSigParam::new("c=3"),
        ];
        assert_eq!(collect_kuaishou_sig_params(&params), vec!["a=1", "c=3"]);
    }

    #[test]
    fn encode_component_matches_encode_uri_component() {
        assert_eq!(encode_component("a b"), "a%20b");
        assert_eq!(encode_component("a!b"), "a%21b");
        assert_eq!(encode_component("a'b"), "a%27b");
        assert_eq!(encode_component("a(b)"), "a%28b%29");
        assert_eq!(encode_component("a*b"), "a%2Ab");
        assert_eq!(encode_component("a~b-c_d.e"), "a~b-c_d.e");
        assert_eq!(encode_component("中文"), "%E4%B8%AD%E6%96%87");
    }
}
