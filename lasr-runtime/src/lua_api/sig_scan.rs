use asr::{Process, future::next_tick};
use tsuki::{
    Value,
    context::{Args, Context, Ret},
};

use crate::state::{Result, State};

#[derive(Copy, Clone)]
struct SigByte {
    value: u8,
    mask: u8,
}

fn parse_sig_token(token: &str) -> Result<SigByte, &'static str> {
    if token == "?" || token == "??" {
        return Ok(SigByte { value: 0, mask: 0 });
    }

    let bytes = token.as_bytes();
    if bytes.len() != 2 {
        return Err("signature token must be 2 hex chars or '?' wildcards");
    }

    let (hi_val, hi_mask) = match bytes[0] {
        b'?' => (0, 0),
        c => (hex_nibble(c)?, 0xF),
    };
    let (lo_val, lo_mask) = match bytes[1] {
        b'?' => (0, 0),
        c => (hex_nibble(c)?, 0xF),
    };

    Ok(SigByte {
        value: (hi_val << 4) | lo_val,
        mask: (hi_mask << 4) | lo_mask,
    })
}

fn hex_nibble(c: u8) -> Result<u8, &'static str> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("signature contains non-hex characters"),
    }
}

fn parse_signature(pattern: &str) -> Result<Vec<SigByte>, &'static str> {
    let mut out = Vec::new();
    for token in pattern.split_whitespace() {
        out.push(parse_sig_token(token)?);
    }
    if out.is_empty() {
        return Err("signature is empty");
    }
    Ok(out)
}

pub async fn sig_scan<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    let signature = {
        let pattern_arg = cx.arg(1);
        let pattern = pattern_arg
            .to_str()?
            .as_utf8()
            .ok_or_else(|| pattern_arg.error("signature is not valid UTF-8"))?
            .to_owned();
        parse_signature(&pattern).map_err(|msg| pattern_arg.error(msg))?
    };

    let offset = {
        let offset_arg = cx.arg(2);
        offset_arg.to_int()?
    };

    let found = {
        let process_ref = cx.associated_data().process.borrow();
        let process = process_ref.as_ref().ok_or("no process attached")?;

        scan_signature(process, &signature, offset).await?
    };

    let base_address = cx.associated_data().base_address.get().value() as i64;

    cx.push(if let Some(address) = found {
        Value::Int(address.wrapping_sub(base_address))
    } else {
        Value::Nil
    })?;
    Ok(cx.into())
}

async fn scan_signature(
    process: &Process,
    signature: &[SigByte],
    offset: i64,
) -> Result<Option<i64>, &'static str> {
    let sig_len = signature.len();
    let chunk_size: usize = 0x10000;
    let mut buf = vec![0u8; chunk_size];
    let mut prev_tail = vec![0u8; sig_len.saturating_sub(1)];
    let mut prev_tail_len: usize;
    let mut window = vec![0u8; chunk_size + sig_len.saturating_sub(1)];
    let matcher = SigMatcher::new(signature);

    let mut chunk_counter: u32 = 0;
    for range in process.memory_ranges() {
        let (base, range_size) = range.range().map_err(|_| "failed to query memory range")?;

        if range_size == 0 {
            continue;
        }

        prev_tail_len = 0;
        let mut offset_bytes: u64 = 0;
        while offset_bytes < range_size {
            let remaining = (range_size - offset_bytes) as usize;
            let read_len = remaining.min(chunk_size);
            let buf_slice = &mut buf[..read_len];

            if process.read_into_buf(base + offset_bytes, buf_slice).is_err() {
                break;
            }

            window[..prev_tail_len].copy_from_slice(&prev_tail[..prev_tail_len]);
            window[prev_tail_len..prev_tail_len + read_len].copy_from_slice(buf_slice);
            let window_len = prev_tail_len + read_len;

            if let Some(found_in_window) = matcher.find_in(&window[..window_len]) {
                let window_start = offset_bytes.saturating_sub(prev_tail_len as u64);
                let found = window_start + found_in_window as u64;
                let address = base.value() as i64 + found as i64 + offset;
                return Ok(Some(address));
            }

            if sig_len > 1 {
                prev_tail_len = (sig_len - 1).min(read_len);
                prev_tail[..prev_tail_len].copy_from_slice(&buf_slice[read_len - prev_tail_len..]);
            }

            offset_bytes += read_len as u64;
            chunk_counter = chunk_counter.wrapping_add(1);
            if chunk_counter.is_multiple_of(64) {
                next_tick().await;
            }
        }
    }

    Ok(None)
}

struct SigMatcher {
    signature: Vec<SigByte>,
    anchor_pos: Option<usize>,
    anchor_byte: u8,
    check_pos: Option<usize>,
    check_byte: u8,
}

impl SigMatcher {
    fn new(signature: &[SigByte]) -> Self {
        let mut exact_positions = Vec::new();
        for (i, sig) in signature.iter().enumerate() {
            if sig.mask == 0xFF {
                exact_positions.push((i, sig.value));
            }
        }

        let (anchor_pos, anchor_byte) = exact_positions
            .first()
            .copied()
            .map_or((None, 0), |(i, b)| (Some(i), b));

        let check = if let Some(anchor_index) = anchor_pos {
            exact_positions
                .iter()
                .filter(|(i, _)| *i != anchor_index)
                .max_by_key(|(i, _)| i.abs_diff(anchor_index))
                .copied()
        } else {
            None
        };

        let (check_pos, check_byte) = check.map_or((None, 0), |(i, b)| (Some(i), b));

        Self {
            signature: signature.to_vec(),
            anchor_pos,
            anchor_byte,
            check_pos,
            check_byte,
        }
    }

    fn find_in(&self, haystack: &[u8]) -> Option<usize> {
        let pat_len = self.signature.len();
        if haystack.len() < pat_len {
            return None;
        }

        if let Some(anchor_pos) = self.anchor_pos {
            let mut search_from = anchor_pos;
            while let Some(anchor_hit) = find_byte_swar(haystack, self.anchor_byte, search_from) {
                let start = anchor_hit - anchor_pos;
                if start + pat_len > haystack.len() {
                    break;
                }

                if let Some(check_pos) = self.check_pos
                    && haystack[start + check_pos] != self.check_byte
                {
                    search_from = anchor_hit + 1;
                    continue;
                }

                if sig_matches_at(haystack, start, &self.signature) {
                    return Some(start);
                }

                search_from = anchor_hit + 1;
            }
            None
        } else {
            (0..=haystack.len() - pat_len)
                .find(|&start| sig_matches_at(haystack, start, &self.signature))
        }
    }
}

fn sig_byte_matches(sig: SigByte, byte: u8) -> bool {
    (byte & sig.mask) == sig.value
}

fn sig_matches_at(haystack: &[u8], start: usize, signature: &[SigByte]) -> bool {
    signature
        .iter()
        .enumerate()
        .all(|(i, sig)| sig_byte_matches(*sig, haystack[start + i]))
}

fn find_byte_swar(haystack: &[u8], needle: u8, mut start: usize) -> Option<usize> {
    if start >= haystack.len() {
        return None;
    }

    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;
    let repeated = (needle as u64) * ONES;

    while start + 8 <= haystack.len() {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&haystack[start..start + 8]);
        let word = u64::from_ne_bytes(bytes);
        let x = word ^ repeated;
        let eq = x.wrapping_sub(ONES) & !x & HIGHS;
        if eq != 0 {
            let index = (eq.trailing_zeros() / 8) as usize;
            return Some(start + index);
        }
        start += 8;
    }

    haystack
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, &b)| (b == needle).then_some(i))
}
