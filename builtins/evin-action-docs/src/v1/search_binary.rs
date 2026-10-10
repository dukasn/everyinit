// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Minimal Perfect Hash (CHD) & Elias-Fano Quasi-Succinct Binary Search Engine.

use std::collections::BTreeMap;

pub const MAGIC: [u8; 4] = [0x45, 0x56, 0x49, 0x4E]; // "EVIN"
pub const MAGIC_CATALOG: [u8; 4] = [0x45, 0x56, 0x43, 0x54]; // "EVCT"
pub const MAGIC_INDEX: [u8; 4] = [0x45, 0x56, 0x49, 0x58]; // "EVIX"
pub const VERSION: u16 = 1;

/// Standard Base64 encoder for inlining binary octet streams.
pub fn to_base64(data: &[u8]) -> String {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[(triple >> 18) & 63] as char);
        out.push(B64[(triple >> 12) & 63] as char);
        if chunk.len() > 1 {
            out.push(B64[(triple >> 6) & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[triple & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Seeded 32-bit FNV-1a hash function identical to JavaScript implementation.
#[inline]
pub fn fnv1a_seeded(data: &str, seed: u32) -> u32 {
    let mut hash = 2166136261u32 ^ seed;
    for byte in data.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// Compress, Hash and Displace (CHD) Minimal Perfect Hash Function builder.
pub struct ChdBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChdTable {
    pub bucket_count: usize,
    pub term_count: usize,
    pub displacements: Vec<u16>,
    pub fingerprints: Vec<u8>,
    pub slot_to_term: Vec<String>,
}

impl ChdBuilder {
    pub fn build(terms: &[String]) -> ChdTable {
        let term_count = terms.len();
        if term_count == 0 {
            return ChdTable {
                bucket_count: 0,
                term_count: 0,
                displacements: Vec::new(),
                fingerprints: Vec::new(),
                slot_to_term: Vec::new(),
            };
        }

        let bucket_count = term_count.div_ceil(4).max(1);
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); bucket_count];

        for (idx, term) in terms.iter().enumerate() {
            let h = fnv1a_seeded(term, 0);
            let b = (h as usize) % bucket_count;
            buckets[b].push(idx);
        }

        // Sort bucket indices descending by item count
        let mut bucket_order: Vec<usize> = (0..bucket_count).collect();
        bucket_order.sort_by(|&a, &b| buckets[b].len().cmp(&buckets[a].len()));

        let mut occupied = vec![false; term_count];
        let mut displacements = vec![0u16; bucket_count];
        let mut slot_to_term_idx = vec![usize::MAX; term_count];

        let mut candidate_slots = Vec::new();

        for &b in &bucket_order {
            let items = &buckets[b];
            if items.is_empty() {
                continue;
            }

            if items.len() == 1 {
                let term_idx = items[0];
                let term = &terms[term_idx];
                for d in 0..u16::MAX {
                    let h = fnv1a_seeded(term, d as u32);
                    let slot = (h as usize) % term_count;
                    if !occupied[slot] {
                        occupied[slot] = true;
                        displacements[b] = d;
                        slot_to_term_idx[slot] = term_idx;
                        break;
                    }
                }
            } else {
                let mut found = false;
                candidate_slots.clear();
                candidate_slots.reserve(items.len());
                for d in 1..u16::MAX {
                    candidate_slots.clear();
                    let mut conflict = false;

                    for &term_idx in items {
                        let term = &terms[term_idx];
                        let h = fnv1a_seeded(term, d as u32);
                        let slot = (h as usize) % term_count;
                        if occupied[slot] || candidate_slots.contains(&slot) {
                            conflict = true;
                            break;
                        }
                        candidate_slots.push(slot);
                    }

                    if !conflict {
                        for (i, &term_idx) in items.iter().enumerate() {
                            let slot = candidate_slots[i];
                            occupied[slot] = true;
                            slot_to_term_idx[slot] = term_idx;
                        }
                        displacements[b] = d;
                        found = true;
                        break;
                    }
                }

                if !found {
                    for &term_idx in items {
                        for slot in 0..term_count {
                            if !occupied[slot] {
                                occupied[slot] = true;
                                slot_to_term_idx[slot] = term_idx;
                                break;
                            }
                        }
                    }
                }
            }
        }

        let mut fingerprints = vec![0u8; term_count];
        let mut slot_to_term = vec![String::new(); term_count];

        for slot in 0..term_count {
            let term_idx = slot_to_term_idx[slot];
            if term_idx < terms.len() {
                let term = &terms[term_idx];
                fingerprints[slot] = (fnv1a_seeded(term, 0) & 0xFF) as u8;
                slot_to_term[slot] = term.clone();
            }
        }

        ChdTable {
            bucket_count,
            term_count,
            displacements,
            fingerprints,
            slot_to_term,
        }
    }

    #[inline]
    pub fn query(table: &ChdTable, term: &str) -> Option<usize> {
        if table.term_count == 0 {
            return None;
        }
        let h0 = fnv1a_seeded(term, 0);
        let b = (h0 as usize) % table.bucket_count;
        let d = table.displacements[b];
        let slot = (fnv1a_seeded(term, d as u32) as usize) % table.term_count;
        if table.fingerprints[slot] == (h0 & 0xFF) as u8 && table.slot_to_term[slot] == term {
            Some(slot)
        } else {
            None
        }
    }
}

/// Elias-Fano Quasi-Succinct Inverted Postings compressor.
pub struct EliasFanoEncoder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedPostingList {
    pub count: u16,
    pub low_bits_width: u8,
    pub low_bits: Vec<u8>,
    pub high_bits: Vec<u8>,
    pub weights: Vec<u8>,
}

impl EliasFanoEncoder {
    pub fn encode(doc_ids: &[usize], weights: &[u8], universe_size: usize) -> EncodedPostingList {
        let count = doc_ids.len() as u16;
        if count == 0 {
            return EncodedPostingList {
                count: 0,
                low_bits_width: 0,
                low_bits: Vec::new(),
                high_bits: Vec::new(),
                weights: Vec::new(),
            };
        }

        let u = universe_size.max(1);
        let m = doc_ids.len();
        let low_bits_width = if u > m {
            (((u as f64) / (m as f64)).log2().floor() as u8).min(16)
        } else {
            0
        };

        // Encode low bits
        let mut low_bitstream = BitWriter::new();
        let mut high_values = Vec::with_capacity(m);

        let mask = (1usize << low_bits_width) - 1;
        for &doc_id in doc_ids {
            let low = (doc_id & mask) as u32;
            low_bitstream.write_bits(low, low_bits_width as usize);
            high_values.push(doc_id >> low_bits_width);
        }

        // Encode high bits in unary bucket progression
        let mut high_bitstream = BitWriter::new();
        let mut current_high = 0usize;
        for &h in &high_values {
            while current_high < h {
                high_bitstream.write_bit(false); // Step to next bucket
                current_high += 1;
            }
            high_bitstream.write_bit(true); // Element in this bucket
        }
        high_bitstream.write_bit(false);

        EncodedPostingList {
            count,
            low_bits_width,
            low_bits: low_bitstream.into_bytes(),
            high_bits: high_bitstream.into_bytes(),
            weights: weights.to_vec(),
        }
    }

    pub fn decode(encoded: &EncodedPostingList) -> Vec<(usize, u8)> {
        let count = encoded.count as usize;
        if count == 0 {
            return Vec::new();
        }

        let low_reader = BitReader::new(&encoded.low_bits);
        let mut high_reader = BitReader::new(&encoded.high_bits);
        let mut results = Vec::with_capacity(count);

        let mut current_high = 0usize;
        let mut elements_read = 0;

        while elements_read < count {
            if let Some(bit) = high_reader.read_bit() {
                if bit {
                    let low = low_reader
                        .read_bits_at(
                            elements_read * (encoded.low_bits_width as usize),
                            encoded.low_bits_width as usize,
                        )
                        .unwrap_or(0);
                    let doc_id = (current_high << encoded.low_bits_width) | (low as usize);
                    let weight = encoded.weights.get(elements_read).copied().unwrap_or(0);
                    results.push((doc_id, weight));
                    elements_read += 1;
                } else {
                    current_high += 1;
                }
            } else {
                break;
            }
        }

        results
    }
}

/// Bitstream serialization helper.
struct BitWriter {
    bytes: Vec<u8>,
    bit_offset: usize,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_offset: 0,
        }
    }

    fn write_bit(&mut self, bit: bool) {
        let byte_idx = self.bit_offset / 8;
        let bit_in_byte = self.bit_offset % 8;
        if byte_idx >= self.bytes.len() {
            self.bytes.push(0);
        }
        if bit {
            self.bytes[byte_idx] |= 1 << (7 - bit_in_byte);
        }
        self.bit_offset += 1;
    }

    fn write_bits(&mut self, value: u32, count: usize) {
        for i in (0..count).rev() {
            let bit = ((value >> i) & 1) != 0;
            self.write_bit(bit);
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Bitstream deserialization helper.
struct BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    fn read_bit(&mut self) -> Option<bool> {
        let byte_idx = self.bit_offset / 8;
        let bit_in_byte = self.bit_offset % 8;
        if byte_idx >= self.bytes.len() {
            return None;
        }
        let bit = (self.bytes[byte_idx] & (1 << (7 - bit_in_byte))) != 0;
        self.bit_offset += 1;
        Some(bit)
    }

    fn read_bits_at(&self, offset: usize, count: usize) -> Option<u32> {
        let mut value = 0u32;
        for i in 0..count {
            let cur_offset = offset + i;
            let byte_idx = cur_offset / 8;
            let bit_in_byte = cur_offset % 8;
            if byte_idx >= self.bytes.len() {
                return None;
            }
            let bit = (self.bytes[byte_idx] & (1 << (7 - bit_in_byte))) != 0;
            if bit {
                value |= 1 << (count - 1 - i);
            }
        }
        Some(value)
    }
}

/// High-performance serializer for `search.bin`.
pub struct BinarySearchIndexWriter;

impl BinarySearchIndexWriter {
    pub fn serialize(
        docs: &[(String, String, String, String, String, usize, u8)],
        snippets: &[String],
        index: &BTreeMap<String, Vec<(usize, u8, u8)>>,
    ) -> Vec<u8> {
        let doc_count = docs.len() as u32;
        let term_count = index.len() as u32;

        let terms: Vec<String> = index.keys().cloned().collect();
        let chd = ChdBuilder::build(&terms);

        let mut buffer = Vec::with_capacity(32 * 1024);

        // 1. Reserve 32-byte header
        buffer.extend_from_slice(&MAGIC);
        buffer.extend_from_slice(&VERSION.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes()); // Flags
        buffer.extend_from_slice(&doc_count.to_le_bytes());
        buffer.extend_from_slice(&term_count.to_le_bytes());

        // Placeholders for section offsets (filled at the end)
        let docs_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes());
        let mphf_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes());
        let postings_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes());
        let strings_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes());

        // String pool builder
        let mut string_pool = Vec::new();
        let mut intern_str = |s: &str| -> (u32, u16) {
            let offset = string_pool.len() as u32;
            let bytes = s.as_bytes();
            string_pool.extend_from_slice(bytes);
            (offset, bytes.len() as u16)
        };

        // Pre-intern snippets
        let mut snippet_offsets = Vec::with_capacity(snippets.len());
        for snippet in snippets {
            snippet_offsets.push(intern_str(snippet));
        }

        // Intern term strings in slot order
        let mut term_offsets = Vec::with_capacity(term_count as usize);
        for slot in 0..term_count as usize {
            let term = chd.slot_to_term.get(slot).map(|s| s.as_str()).unwrap_or("");
            term_offsets.push(intern_str(term));
        }

        // 2. Section 1: Doc Descriptors Table (Exact 40 bytes per doc)
        Self::align_4(&mut buffer);
        let docs_offset = buffer.len() as u32;

        for (slug, page_title, section_title, anchor, ref_id, snippet_idx, kind) in docs {
            let (slug_off, slug_len) = intern_str(slug);
            let (page_title_off, page_title_len) = intern_str(page_title);
            let (sec_title_off, sec_title_len) = intern_str(section_title);
            let (anchor_off, anchor_len) = intern_str(anchor);
            let (ref_id_off, ref_id_len) = intern_str(ref_id);
            let (snippet_off, snippet_len) =
                snippet_offsets.get(*snippet_idx).copied().unwrap_or((0, 0));

            buffer.extend_from_slice(&slug_off.to_le_bytes()); // 0..4 (4B)
            buffer.extend_from_slice(&slug_len.to_le_bytes()); // 4..6 (2B)
            buffer.extend_from_slice(&page_title_off.to_le_bytes()); // 6..10 (4B)
            buffer.extend_from_slice(&page_title_len.to_le_bytes()); // 10..12 (2B)
            buffer.extend_from_slice(&sec_title_off.to_le_bytes()); // 12..16 (4B)
            buffer.extend_from_slice(&sec_title_len.to_le_bytes()); // 16..18 (2B)
            buffer.extend_from_slice(&anchor_off.to_le_bytes()); // 18..22 (4B)
            buffer.extend_from_slice(&anchor_len.to_le_bytes()); // 22..24 (2B)
            buffer.extend_from_slice(&ref_id_off.to_le_bytes()); // 24..28 (4B)
            buffer.extend_from_slice(&ref_id_len.to_le_bytes()); // 28..30 (2B)
            buffer.extend_from_slice(&snippet_off.to_le_bytes()); // 30..34 (4B)
            buffer.extend_from_slice(&snippet_len.to_le_bytes()); // 34..36 (2B)
            buffer.push(*kind); // 36..37 (1B)
            buffer.push(0); // 37..38 (1B)
            buffer.push(0); // 38..39 (1B)
            buffer.push(0); // 39..40 (1B) -> Total 40 bytes
        }

        // 3. Section 2: MPHF, Fingerprints, Term String Pointers & Postings Offsets
        Self::align_4(&mut buffer);
        let mphf_offset = buffer.len() as u32;

        buffer.extend_from_slice(&(chd.bucket_count as u32).to_le_bytes());
        for &d in &chd.displacements {
            buffer.extend_from_slice(&d.to_le_bytes());
        }
        Self::align_4(&mut buffer);
        for &fp in &chd.fingerprints {
            buffer.push(fp);
        }
        Self::align_4(&mut buffer);

        // Term string offsets & lengths table (for prefix scans)
        for (off, len) in &term_offsets {
            buffer.extend_from_slice(&off.to_le_bytes());
            buffer.extend_from_slice(&len.to_le_bytes());
            buffer.extend_from_slice(&0u16.to_le_bytes()); // padding to 8 bytes per term entry
        }

        // Placeholders for postings offsets
        let postings_offset_table_pos = buffer.len();
        for _ in 0..term_count {
            buffer.extend_from_slice(&0u32.to_le_bytes());
        }

        // 4. Section 3: Postings Section (Elias-Fano Encoded)
        Self::align_4(&mut buffer);
        let postings_offset = buffer.len() as u32;
        let mut posting_offsets = Vec::with_capacity(term_count as usize);

        for slot in 0..term_count as usize {
            let rel_offset = (buffer.len() as u32) - postings_offset;
            posting_offsets.push(rel_offset);

            if let Some(term) = chd.slot_to_term.get(slot)
                && let Some(postings) = index.get(term)
            {
                let mut sorted_postings = postings.clone();
                sorted_postings.sort_by_key(|p| p.0);

                let doc_ids: Vec<usize> = sorted_postings.iter().map(|p| p.0).collect();
                let weights: Vec<u8> = sorted_postings
                    .iter()
                    .map(|p| {
                        let flags = p.1 & 0x0F;
                        let freq = p.2.min(15);
                        (flags << 4) | freq
                    })
                    .collect();

                let encoded = EliasFanoEncoder::encode(&doc_ids, &weights, doc_count as usize);

                buffer.extend_from_slice(&encoded.count.to_le_bytes());
                buffer.push(encoded.low_bits_width);
                buffer.push(encoded.low_bits.len() as u8);
                buffer.extend_from_slice(&(encoded.high_bits.len() as u16).to_le_bytes());
                buffer.extend_from_slice(&encoded.low_bits);
                buffer.extend_from_slice(&encoded.high_bits);
                buffer.extend_from_slice(&encoded.weights);
                Self::align_4(&mut buffer);
            } else {
                buffer.extend_from_slice(&0u16.to_le_bytes());
                Self::align_4(&mut buffer);
            }
        }

        // Back-fill postings offsets table in Section 2
        for (i, &off) in posting_offsets.iter().enumerate() {
            let pos = postings_offset_table_pos + (i * 4);
            buffer[pos..pos + 4].copy_from_slice(&off.to_le_bytes());
        }

        // 5. Section 4: Strings Pool
        Self::align_4(&mut buffer);
        let strings_offset = buffer.len() as u32;
        buffer.extend_from_slice(&string_pool);

        // 6. Write final section offsets in Header
        buffer[docs_offset_pos..docs_offset_pos + 4].copy_from_slice(&docs_offset.to_le_bytes());
        buffer[mphf_offset_pos..mphf_offset_pos + 4].copy_from_slice(&mphf_offset.to_le_bytes());
        buffer[postings_offset_pos..postings_offset_pos + 4]
            .copy_from_slice(&postings_offset.to_le_bytes());
        buffer[strings_offset_pos..strings_offset_pos + 4]
            .copy_from_slice(&strings_offset.to_le_bytes());

        buffer
    }

    pub fn serialize_catalog(
        docs: &[(String, String, String, String, String, usize, u8)],
        snippets: &[String],
        prompts: &[String],
    ) -> Vec<u8> {
        let doc_count = docs.len() as u32;
        let prompt_count = prompts.len() as u32;

        let mut buffer = Vec::with_capacity(32 * 1024);

        // 1. Reserve 32-byte header
        buffer.extend_from_slice(&MAGIC_CATALOG);
        buffer.extend_from_slice(&VERSION.to_le_bytes()); // 4..6
        buffer.extend_from_slice(&0u16.to_le_bytes()); // Flags 6..8
        buffer.extend_from_slice(&doc_count.to_le_bytes()); // 8..12
        buffer.extend_from_slice(&prompt_count.to_le_bytes()); // 12..16

        let docs_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 16..20
        let prompts_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 20..24
        let strings_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 24..28
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 28..32 (reserved)

        // String pool
        let mut string_pool = Vec::new();
        let mut intern_str = |s: &str| -> (u32, u16) {
            let offset = string_pool.len() as u32;
            let bytes = s.as_bytes();
            string_pool.extend_from_slice(bytes);
            (offset, bytes.len() as u16)
        };

        // Pre-intern snippets
        let mut snippet_offsets = Vec::with_capacity(snippets.len());
        for snippet in snippets {
            snippet_offsets.push(intern_str(snippet));
        }

        // Section 1: Doc Descriptors Table (Exact 40 bytes per doc)
        Self::align_4(&mut buffer);
        let docs_offset = buffer.len() as u32;

        for (slug, page_title, section_title, anchor, ref_id, snippet_idx, kind) in docs {
            let (slug_off, slug_len) = intern_str(slug);
            let (page_title_off, page_title_len) = intern_str(page_title);
            let (sec_title_off, sec_title_len) = intern_str(section_title);
            let (anchor_off, anchor_len) = intern_str(anchor);
            let (ref_id_off, ref_id_len) = intern_str(ref_id);
            let (snippet_off, snippet_len) =
                snippet_offsets.get(*snippet_idx).copied().unwrap_or((0, 0));

            buffer.extend_from_slice(&slug_off.to_le_bytes());
            buffer.extend_from_slice(&slug_len.to_le_bytes());
            buffer.extend_from_slice(&page_title_off.to_le_bytes());
            buffer.extend_from_slice(&page_title_len.to_le_bytes());
            buffer.extend_from_slice(&sec_title_off.to_le_bytes());
            buffer.extend_from_slice(&sec_title_len.to_le_bytes());
            buffer.extend_from_slice(&anchor_off.to_le_bytes());
            buffer.extend_from_slice(&anchor_len.to_le_bytes());
            buffer.extend_from_slice(&ref_id_off.to_le_bytes());
            buffer.extend_from_slice(&ref_id_len.to_le_bytes());
            buffer.extend_from_slice(&snippet_off.to_le_bytes());
            buffer.extend_from_slice(&snippet_len.to_le_bytes());
            buffer.push(*kind);
            buffer.push(0);
            buffer.push(0);
            buffer.push(0);
        }

        // Section 2: Prompts Table
        Self::align_4(&mut buffer);
        let prompts_offset = buffer.len() as u32;
        for prompt in prompts {
            let (off, len) = intern_str(prompt);
            buffer.extend_from_slice(&off.to_le_bytes());
            buffer.extend_from_slice(&len.to_le_bytes());
            buffer.extend_from_slice(&0u16.to_le_bytes());
        }

        // Section 3: Strings Pool
        Self::align_4(&mut buffer);
        let strings_offset = buffer.len() as u32;
        buffer.extend_from_slice(&string_pool);

        // Fill Header offsets
        buffer[docs_offset_pos..docs_offset_pos + 4].copy_from_slice(&docs_offset.to_le_bytes());
        buffer[prompts_offset_pos..prompts_offset_pos + 4]
            .copy_from_slice(&prompts_offset.to_le_bytes());
        buffer[strings_offset_pos..strings_offset_pos + 4]
            .copy_from_slice(&strings_offset.to_le_bytes());

        buffer
    }

    pub fn serialize_index(
        doc_count: usize,
        index: &BTreeMap<String, Vec<(usize, u8, u8)>>,
    ) -> Vec<u8> {
        let term_count = index.len() as u32;
        let terms: Vec<String> = index.keys().cloned().collect();
        let chd = ChdBuilder::build(&terms);

        let mut buffer = Vec::with_capacity(32 * 1024);

        // 1. Reserve 32-byte header
        buffer.extend_from_slice(&MAGIC_INDEX);
        buffer.extend_from_slice(&VERSION.to_le_bytes()); // 4..6
        buffer.extend_from_slice(&0u16.to_le_bytes()); // Flags 6..8
        buffer.extend_from_slice(&(doc_count as u32).to_le_bytes()); // 8..12
        buffer.extend_from_slice(&term_count.to_le_bytes()); // 12..16

        let mphf_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 16..20
        let postings_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 20..24
        let strings_offset_pos = buffer.len();
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 24..28
        buffer.extend_from_slice(&0u32.to_le_bytes()); // 28..32 (reserved)

        // String pool builder for term strings
        let mut string_pool = Vec::new();
        let mut intern_str = |s: &str| -> (u32, u16) {
            let offset = string_pool.len() as u32;
            let bytes = s.as_bytes();
            string_pool.extend_from_slice(bytes);
            (offset, bytes.len() as u16)
        };

        let mut term_offsets = Vec::with_capacity(term_count as usize);
        for slot in 0..term_count as usize {
            let term = chd.slot_to_term.get(slot).map(|s| s.as_str()).unwrap_or("");
            term_offsets.push(intern_str(term));
        }

        // Section 1: MPHF, Fingerprints, Term String Pointers & Postings Offsets
        Self::align_4(&mut buffer);
        let mphf_offset = buffer.len() as u32;

        buffer.extend_from_slice(&(chd.bucket_count as u32).to_le_bytes());
        for &d in &chd.displacements {
            buffer.extend_from_slice(&d.to_le_bytes());
        }
        Self::align_4(&mut buffer);
        for &fp in &chd.fingerprints {
            buffer.push(fp);
        }
        Self::align_4(&mut buffer);

        for (off, len) in &term_offsets {
            buffer.extend_from_slice(&off.to_le_bytes());
            buffer.extend_from_slice(&len.to_le_bytes());
            buffer.extend_from_slice(&0u16.to_le_bytes());
        }

        let postings_offset_table_pos = buffer.len();
        for _ in 0..term_count {
            buffer.extend_from_slice(&0u32.to_le_bytes());
        }

        // Section 2: Postings Section (Elias-Fano Encoded)
        Self::align_4(&mut buffer);
        let postings_offset = buffer.len() as u32;
        let mut posting_offsets = Vec::with_capacity(term_count as usize);

        for slot in 0..term_count as usize {
            let rel_offset = (buffer.len() as u32) - postings_offset;
            posting_offsets.push(rel_offset);

            if let Some(term) = chd.slot_to_term.get(slot)
                && let Some(postings) = index.get(term)
            {
                let mut sorted_postings = postings.clone();
                sorted_postings.sort_by_key(|p| p.0);

                let doc_ids: Vec<usize> = sorted_postings.iter().map(|p| p.0).collect();
                let weights: Vec<u8> = sorted_postings
                    .iter()
                    .map(|p| {
                        let flags = p.1 & 0x0F;
                        let freq = p.2.min(15);
                        (flags << 4) | freq
                    })
                    .collect();

                let encoded = EliasFanoEncoder::encode(&doc_ids, &weights, doc_count);

                buffer.extend_from_slice(&encoded.count.to_le_bytes());
                buffer.push(encoded.low_bits_width);
                buffer.push(encoded.low_bits.len() as u8);
                buffer.extend_from_slice(&(encoded.high_bits.len() as u16).to_le_bytes());
                buffer.extend_from_slice(&encoded.low_bits);
                buffer.extend_from_slice(&encoded.high_bits);
                buffer.extend_from_slice(&encoded.weights);
                Self::align_4(&mut buffer);
            } else {
                buffer.extend_from_slice(&0u16.to_le_bytes());
                Self::align_4(&mut buffer);
            }
        }

        for (i, &off) in posting_offsets.iter().enumerate() {
            let pos = postings_offset_table_pos + (i * 4);
            buffer[pos..pos + 4].copy_from_slice(&off.to_le_bytes());
        }

        // Section 3: Strings Pool
        Self::align_4(&mut buffer);
        let strings_offset = buffer.len() as u32;
        buffer.extend_from_slice(&string_pool);

        // Fill Header offsets
        buffer[mphf_offset_pos..mphf_offset_pos + 4].copy_from_slice(&mphf_offset.to_le_bytes());
        buffer[postings_offset_pos..postings_offset_pos + 4]
            .copy_from_slice(&postings_offset.to_le_bytes());
        buffer[strings_offset_pos..strings_offset_pos + 4]
            .copy_from_slice(&strings_offset.to_le_bytes());

        buffer
    }

    #[inline]
    fn align_4(vec: &mut Vec<u8>) {
        let remainder = vec.len() % 4;
        if remainder != 0 {
            for _ in 0..(4 - remainder) {
                vec.push(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_seeded_deterministic() {
        assert_eq!(fnv1a_seeded("install", 0), fnv1a_seeded("install", 0));
        assert_ne!(fnv1a_seeded("install", 0), fnv1a_seeded("install", 1));
        assert_ne!(fnv1a_seeded("install", 0), fnv1a_seeded("config", 0));
    }

    #[test]
    fn test_chd_builder_and_query() {
        let terms = vec![
            "install".to_string(),
            "config".to_string(),
            "workspace".to_string(),
            "pipeline".to_string(),
            "archive".to_string(),
            "ref-1000".to_string(),
            "ref-4500".to_string(),
        ];

        let table = ChdBuilder::build(&terms);
        assert_eq!(table.term_count, terms.len());

        for term in &terms {
            let slot = ChdBuilder::query(&table, term);
            assert!(slot.is_some(), "Term '{}' should be found", term);
            assert_eq!(table.slot_to_term[slot.unwrap()], *term);
        }

        assert!(ChdBuilder::query(&table, "nonexistent").is_none());
        assert!(ChdBuilder::query(&table, "unknown_term").is_none());
    }

    #[test]
    fn test_elias_fano_roundtrip() {
        let doc_ids = vec![3, 12, 14, 29, 31];
        let weights = vec![0x11, 0x22, 0x33, 0x44, 0x55];
        let universe = 32;

        let encoded = EliasFanoEncoder::encode(&doc_ids, &weights, universe);
        let decoded = EliasFanoEncoder::decode(&encoded);

        assert_eq!(decoded.len(), doc_ids.len());
        for (i, (d, w)) in decoded.iter().enumerate() {
            assert_eq!(*d, doc_ids[i]);
            assert_eq!(*w, weights[i]);
        }
    }

    #[test]
    fn test_binary_search_index_writer_serialization() {
        let docs = vec![
            (
                "getting-started/installation".to_string(),
                "Installation".to_string(),
                "Prerequisites".to_string(),
                "prereqs".to_string(),
                "REF-1000".to_string(),
                0,
                0,
            ),
            (
                "reference/workspace-configuration".to_string(),
                "Workspace configuration".to_string(),
                "Schema".to_string(),
                "schema".to_string(),
                "REF-4500".to_string(),
                1,
                2,
            ),
        ];

        let snippets = vec![
            "Install the standalone EVIN binary...".to_string(),
            "Configure a workspace in .evin/workspace.flow...".to_string(),
        ];

        let mut index = BTreeMap::new();
        index.insert("install".to_string(), vec![(0, 2, 1)]);
        index.insert("workspace".to_string(), vec![(1, 2, 2)]);
        index.insert("ref-4500".to_string(), vec![(1, 1, 1)]);

        let bytes = BinarySearchIndexWriter::serialize(&docs, &snippets, &index);

        assert_eq!(&bytes[0..4], &MAGIC);
        assert!(bytes.len() > 32);

        let prompts = vec!["Search documentation...".to_string()];
        let cat_bytes = BinarySearchIndexWriter::serialize_catalog(&docs, &snippets, &prompts);
        assert_eq!(&cat_bytes[0..4], &MAGIC_CATALOG);
        assert!(cat_bytes.len() > 32);

        let idx_bytes = BinarySearchIndexWriter::serialize_index(docs.len(), &index);
        assert_eq!(&idx_bytes[0..4], &MAGIC_INDEX);
        assert!(idx_bytes.len() > 32);
    }
}
