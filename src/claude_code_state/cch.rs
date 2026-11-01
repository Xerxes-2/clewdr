//! Final-byte billing checksum shared with gproxy's Claude Code channel.
//! Hash with the placeholder intact, using the native compact-byte scanners
//! for model contents, fallback arrays, token limits and fallback credit tokens.
//! Applied after all request shaping, to the exact bytes sent upstream.
use anthropic_wire::CreateMessageParams;
use xxhash_rust::xxh64::Xxh64;

const SEED: u64 = 0x4d65_9218_e32a_3268;
const PLACEHOLDER: &[u8] = b"cch=00000";

pub(super) fn serialize(body: &CreateMessageParams) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec(body)?;
    // Credit replays retain the checksum of the request that earned the credit.
    if body.fallback_credit_token.is_none() {
        patch(&mut bytes);
    }
    Ok(bytes)
}

fn patch(bytes: &mut [u8]) {
    // Native transport searches only the first 300 bytes after the first
    // compact array-form system key; it does not parse the billing block.
    if let Some(system) = find(bytes, b"\"system\":[") {
        let end = bytes.len().min(system + 300);
        if let Some(offset) = find(&bytes[system..end], PLACEHOLDER) {
            let pos = system + offset;
            let hex = format!("{:05x}", checksum(bytes));
            bytes[pos + 4..pos + 9].copy_from_slice(hex.as_bytes());
        }
    }
}

fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|w| w == needle)
}

/// Native transport scans compact byte patterns, rather than JSON values.
/// Return the earliest complete exclusion at or after `start`. Matching and
/// delimiter behavior deliberately follow 2.1.288, including partial numbers
/// and the simple quote scan for fallback credit tokens.
fn exclusion(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut earliest = None;
    let mut candidate = |from: usize, mut end: usize| {
        let mut from = from;
        if bytes.get(end) == Some(&b',') {
            end += 1;
        } else if from > start && bytes[from - 1] == b',' {
            from -= 1;
        }
        if earliest.is_none_or(|(previous, _)| from < previous) {
            earliest = Some((from, end));
        }
    };
    if let Some(offset) = find(&bytes[start..], b"\"fallbacks\":[") {
        let from = start + offset;
        let mut end = from + 13;
        let mut depth = 1_u32;
        let mut string = false;
        while end < bytes.len() {
            match bytes[end] {
                b'\\' if string && end + 1 < bytes.len() => {
                    end += 2;
                    continue;
                }
                b'"' => string = !string,
                b'[' if !string => depth += 1,
                b']' if !string => {
                    depth -= 1;
                    if depth == 0 {
                        candidate(from, end + 1);
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
    }
    if let Some(offset) = find(&bytes[start..], b"\"fallback_credit_token\":\"") {
        let from = start + offset;
        if let Some(end) = bytes[from + 25..].iter().position(|b| *b == b'"') {
            candidate(from, from + 25 + end + 1);
        }
    }
    let mut search = start;
    while let Some(offset) = find(&bytes[search..], b"\"max_tokens\":") {
        let from = search + offset;
        search = from + 13;
        let digits = bytes[search..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits > 0 {
            candidate(from, search + digits);
        }
    }
    earliest
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = Xxh64::new(SEED);
    let mut cursor = 0;
    let mut next = exclusion(bytes, cursor);
    loop {
        let model = find(&bytes[cursor..], b"\"model\":\"").map(|pos| cursor + pos + 9);
        if let Some((from, end)) = next
            && model.is_none_or(|pos| from <= pos)
        {
            hash.update(&bytes[cursor..from]);
            cursor = end;
            next = exclusion(bytes, cursor);
        } else if let Some(from) = model
            && let Some(end) = bytes[from..].iter().position(|b| *b == b'"')
        {
            hash.update(&bytes[cursor..from]);
            cursor = from + end;
        } else {
            hash.update(&bytes[cursor..]);
            break;
        }
    }
    hash.digest() & 0xf_ffff
}
