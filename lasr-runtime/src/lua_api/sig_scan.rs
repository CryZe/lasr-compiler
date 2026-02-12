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
    let lps = build_lps(signature);

    let mut chunk_counter: u32 = 0;
    for range in process.memory_ranges() {
        let (base, range_size) = range.range().map_err(|_| "failed to query memory range")?;

        if range_size == 0 {
            continue;
        }
        let mut offset_bytes: u64 = 0;
        let mut matched: usize = 0;
        while offset_bytes < range_size {
            let remaining = (range_size - offset_bytes) as usize;
            let read_len = remaining.min(chunk_size);
            let buf_slice = &mut buf[..read_len];

            if process.read_into_buf(base + offset_bytes, buf_slice).is_err() {
                break;
            }

            for (i, &byte) in buf_slice.iter().enumerate() {
                while matched > 0 && !sig_byte_matches(signature[matched], byte) {
                    matched = lps[matched - 1];
                }
                if sig_byte_matches(signature[matched], byte) {
                    matched += 1;
                    if matched == sig_len {
                        let found = offset_bytes + i as u64 + 1 - sig_len as u64;
                        let address = base.value() as i64 + found as i64 + offset;
                        return Ok(Some(address));
                    }
                }
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

fn build_lps(signature: &[SigByte]) -> Vec<usize> {
    let mut lps = vec![0usize; signature.len()];
    let mut len = 0;

    for i in 1..signature.len() {
        while len > 0 && !sig_byte_eq(signature[i], signature[len]) {
            len = lps[len - 1];
        }
        if sig_byte_eq(signature[i], signature[len]) {
            len += 1;
            lps[i] = len;
        }
    }

    lps
}

fn sig_byte_matches(sig: SigByte, byte: u8) -> bool {
    (byte & sig.mask) == sig.value
}

fn sig_byte_eq(left: SigByte, right: SigByte) -> bool {
    left.value == right.value && left.mask == right.mask
}
