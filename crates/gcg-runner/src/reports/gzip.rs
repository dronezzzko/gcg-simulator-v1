//! An in-house gzip writer (RFC 1952 around RFC 1951 DEFLATE with the fixed Huffman codes) in
//! safe Rust, for `gcg-eval benchmark --compress` in builds without the `zstd` encoder (W-99,
//! docs/dev/PLAN.md §12.2 P6-M). `gzip -dc` gives back the input bytes exactly.
//!
//! Matching is LZ77 over a 32 KiB window with hash chains on 3-byte prefixes, a bounded chain
//! walk and lazy evaluation off: a deterministic function of the input, so the same report
//! always compresses to the same bytes. The header carries no file name and `MTIME = 0`.

/// The DEFLATE window (RFC 1951 §2: distances up to 32768).
const WINDOW: usize = 32 * 1024;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// Hash-chain links followed per position: bounds the work per byte.
const MAX_CHAIN: usize = 64;
const HASH_BITS: u32 = 15;
/// Bytes per DEFLATE block (each block is closed and a new one opened).
const BLOCK: usize = 1 << 16;

/// The CRC-32 of RFC 1952 §8 (reflected polynomial 0xEDB88320).
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0_u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = u32::try_from(i).expect("below 256");
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFF_u32;
    for &b in bytes {
        crc = table[usize::from(u8::try_from((crc ^ u32::from(b)) & 0xFF).expect("masked"))]
            ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Bits written least-significant first, as DEFLATE packs them.
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    const fn new(out: Vec<u8>) -> Self {
        Self { out, acc: 0, n: 0 }
    }

    fn bits(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out
                .push(u8::try_from(self.acc & 0xFF).expect("masked to a byte"));
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, which DEFLATE stores most-significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let mut reversed = 0_u32;
        for i in 0..len {
            reversed |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.bits(reversed, len);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out
                .push(u8::try_from(self.acc & 0xFF).expect("masked to a byte"));
        }
        self.out
    }
}

/// The fixed literal/length code of RFC 1951 §3.2.6.
fn literal(w: &mut BitWriter, symbol: u32) {
    match symbol {
        0..=143 => w.code(0x30 + symbol, 8),
        144..=255 => w.code(0x190 + symbol - 144, 9),
        256..=279 => w.code(symbol - 256, 7),
        _ => w.code(0xC0 + symbol - 280, 8),
    }
}

/// `(base, extra bits)` of the length codes 257..=285.
const LENGTHS: [(u32, u32); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// `(base, extra bits)` of the distance codes 0..=29.
const DISTANCES: [(u32, u32); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// The last table entry whose base is at most `value`.
fn bucket(table: &[(u32, u32)], value: u32) -> usize {
    table
        .iter()
        .rposition(|&(base, _)| base <= value)
        .unwrap_or(0)
}

fn emit_match(w: &mut BitWriter, length: usize, distance: usize) {
    let length = u32::try_from(length).expect("at most 258");
    let distance = u32::try_from(distance).expect("at most 32768");
    let li = bucket(&LENGTHS, length);
    let (lbase, lextra) = LENGTHS[li];
    literal(w, 257 + u32::try_from(li).expect("below 29"));
    // Length 258 has its own code (285) with no extra bits, though 227 + 31 would also fit.
    if lextra > 0 {
        w.bits(length - lbase, lextra);
    }
    let di = bucket(&DISTANCES, distance);
    let (dbase, dextra) = DISTANCES[di];
    w.code(u32::try_from(di).expect("below 30"), 5);
    if dextra > 0 {
        w.bits(distance - dbase, dextra);
    }
}

fn hash3(data: &[u8], i: usize) -> usize {
    let v = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8) | u32::from(data[i + 2]);
    usize::try_from(v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)).expect("15 bits")
}

/// Raw DEFLATE (RFC 1951) of `data` with fixed Huffman blocks.
#[must_use]
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::new(Vec::with_capacity(data.len() / 3 + 16));
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; data.len().max(1)];
    let insert = |head: &mut [usize], prev: &mut [usize], i: usize| {
        if i + MIN_MATCH <= data.len() {
            let h = hash3(data, i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    let mut start = 0;
    loop {
        // One block per BLOCK bytes; a match never runs past its block's end, while its
        // distance may reach back into earlier blocks (RFC 1951 §3.2).
        let end = (start + BLOCK).min(data.len());
        let last = end == data.len();
        w.bits(u32::from(last), 1);
        w.bits(1, 2);
        let mut i = start;
        while i < end {
            let mut best_len = 0;
            let mut best_dist = 0;
            if i + MIN_MATCH <= end {
                let mut candidate = head[hash3(data, i)];
                let mut chain = 0;
                let limit = (end - i).min(MAX_MATCH);
                while candidate != usize::MAX && i - candidate <= WINDOW && chain < MAX_CHAIN {
                    let mut len = 0;
                    while len < limit && data[candidate + len] == data[i + len] {
                        len += 1;
                    }
                    if len > best_len {
                        best_len = len;
                        best_dist = i - candidate;
                        if len == limit {
                            break;
                        }
                    }
                    candidate = prev[candidate];
                    chain += 1;
                }
            }
            if best_len >= MIN_MATCH {
                emit_match(&mut w, best_len, best_dist);
                for k in i..i + best_len {
                    insert(&mut head, &mut prev, k);
                }
                i += best_len;
            } else {
                literal(&mut w, u32::from(data[i]));
                insert(&mut head, &mut prev, i);
                i += 1;
            }
        }
        literal(&mut w, 256);
        if last {
            break;
        }
        start = end;
    }
    w.finish()
}

/// A gzip member (RFC 1952) holding `data`: header (no name, `MTIME = 0`, `XFL = 0`,
/// `OS = 255` unknown), the fixed-Huffman DEFLATE stream, CRC-32 and `ISIZE`.
#[must_use]
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0xFF];
    out.extend(deflate(data));
    out.extend(crc32(data).to_le_bytes());
    let size = u32::try_from(data.len() % (1 << 32)).expect("reduced modulo 2^32");
    out.extend(size.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn the_empty_input_is_one_final_block() {
        // BFINAL=1, BTYPE=01, end-of-block 0000000: 0x03 0x00.
        assert_eq!(deflate(b""), [0x03, 0x00]);
        let g = gzip(b"");
        assert_eq!(&g[..3], &[0x1F, 0x8B, 8]);
        assert_eq!(g.len(), 10 + 2 + 8);
    }

    #[test]
    fn repeats_compress() {
        let text = "{\"a\":1}\n".repeat(1000);
        assert!(gzip(text.as_bytes()).len() < text.len() / 10);
    }
}
