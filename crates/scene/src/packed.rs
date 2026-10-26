/// Tightly packed palette indices in the litematic `BlockStates` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedIndices {
    bits: u8,
    longs: Vec<i64>,
    volume: u64,
}

impl PackedIndices {
    /// Minimum bit width used by litematic, even for a single palette entry.
    pub const MIN_BITS: u8 = 2;

    pub fn new(bits: u8, longs: Vec<i64>, volume: u64) -> Option<Self> {
        if bits == 0 || bits > 32 {
            return None;
        }

        let expected = Self::expected_long_count(volume, bits)?;
        if longs.len() as u64 != expected {
            return None;
        }

        Some(Self {
            bits,
            longs,
            volume,
        })
    }

    pub fn bits_for_palette(palette_len: usize) -> u8 {
        assert!(palette_len > 0, "palette must not be empty");
        let required = (usize::BITS - (palette_len - 1).leading_zeros()) as u8;
        required.max(Self::MIN_BITS)
    }

    pub fn expected_long_count(volume: u64, bits: u8) -> Option<u64> {
        let total_bits = volume.checked_mul(u64::from(bits))?;
        Some(total_bits.div_ceil(64))
    }

    /// Pack a slice of values using the litematic layout. Useful for fixtures.
    pub fn from_values(bits: u8, values: &[u32]) -> Option<Self> {
        if bits == 0 || bits > 32 {
            return None;
        }

        let volume = u64::try_from(values.len()).ok()?;
        let long_count = Self::expected_long_count(volume, bits)?;
        let mut longs = vec![0i64; usize::try_from(long_count).ok()?];
        let mask = (1u64 << bits) - 1;

        for (index, value) in values.iter().enumerate() {
            if u64::from(*value) > mask {
                return None;
            }
            set_at(&mut longs, bits, index as u64, *value);
        }

        Self::new(bits, longs, volume)
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }

    pub fn longs(&self) -> &[i64] {
        &self.longs
    }

    pub fn volume(&self) -> u64 {
        self.volume
    }

    /// Read one entry. `index` must be lower than `volume`.
    pub fn get(&self, index: u64) -> u32 {
        assert!(index < self.volume, "packed index out of range");

        let bits = u64::from(self.bits);
        let start_offset = index * bits;
        let start_array = (start_offset / 64) as usize;
        let end_array = (((index + 1) * bits) - 1) / 64;
        let start_bit = (start_offset % 64) as u32;
        let mask = (1u64 << self.bits) - 1;
        let first = self.longs[start_array] as u64;

        if start_array == end_array as usize {
            ((first >> start_bit) & mask) as u32
        } else {
            let second = self.longs[end_array as usize] as u64;
            (((first >> start_bit) | (second << (64 - start_bit))) & mask) as u32
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.cursor()
    }

    /// Sequential decoder with a one-word buffer. Random `get` calls recompute
    /// the word and bit offset for every index; scanning uses this cursor.
    pub(crate) fn cursor(&self) -> PackedCursor<'_> {
        PackedCursor {
            states: self,
            remaining: self.volume,
            next_long: 0,
            buffer: 0,
            buffered: 0,
        }
    }
}

pub(crate) struct PackedCursor<'a> {
    states: &'a PackedIndices,
    remaining: u64,
    next_long: usize,
    buffer: u64,
    buffered: u32,
}

impl Iterator for PackedCursor<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let bits = u32::from(self.states.bits);
        let mask = (1u64 << bits) - 1;
        let value = if self.buffered >= bits {
            let value = (self.buffer & mask) as u32;
            self.buffer >>= bits;
            self.buffered -= bits;
            value
        } else {
            let word = self.states.longs.get(self.next_long).copied().unwrap_or(0) as u64;
            self.next_long += 1;

            let low = self.buffered;
            let need = bits - low;
            let low_mask = (1u64 << low) - 1;
            let need_mask = (1u64 << need) - 1;
            let value = ((self.buffer & low_mask) | ((word & need_mask) << low)) as u32;
            self.buffer = word >> need;
            self.buffered = 64 - need;
            value
        };

        self.remaining -= 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::try_from(self.remaining).unwrap_or(usize::MAX);
        (remaining, Some(remaining))
    }
}

fn set_at(longs: &mut [i64], bits: u8, index: u64, value: u32) {
    let bits = u64::from(bits);
    let start_offset = index * bits;
    let start_array = (start_offset / 64) as usize;
    let end_array = (((index + 1) * bits) - 1) / 64;
    let start_bit = (start_offset % 64) as u32;
    let mask = (1u64 << bits) - 1;
    let value = u64::from(value);

    let first = longs[start_array] as u64;
    let cleared = first & !(mask << start_bit);
    longs[start_array] = (cleared | ((value & mask) << start_bit)) as i64;

    if start_array != end_array as usize {
        let end_offset = 64 - start_bit;
        let high_bits = bits - u64::from(end_offset);
        let second = longs[end_array as usize] as u64;
        let shifted = (second >> high_bits) << high_bits;
        let tail = (value & mask) >> end_offset;
        longs[end_array as usize] = (shifted | tail) as i64;
    }
}

#[cfg(test)]
mod tests {
    use super::PackedIndices;

    #[test]
    fn cursor_matches_random_access() {
        for bits in 2..=12 {
            let count = 1000;
            let mask = (1u32 << bits) - 1;
            let values: Vec<u32> = (0..count).map(|index| (index as u32 * 37) & mask).collect();
            let packed = PackedIndices::from_values(bits, &values).unwrap();
            let decoded: Vec<u32> = packed.iter().collect();
            assert_eq!(decoded, values, "bits={bits}");
        }
    }
}
