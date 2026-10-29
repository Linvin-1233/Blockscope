/// Resolve a premultiplied internal framebuffer to straight-alpha output.
///
/// The GPU blends premultiplied fragments so translucent draws match the CPU's
/// source-over accumulation. Supersampled pixels are box-filtered before the
/// premultiplied color is divided by alpha, which is the same order the CPU
/// downsampler uses.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve(
    premultiplied: &[u8],
    row_stride: usize,
    internal_width: u32,
    output_width: u32,
    output_height: u32,
    factor: u32,
    threads: usize,
) -> Vec<u8> {
    let mut output = vec![0u8; (u64::from(output_width) * u64::from(output_height) * 4) as usize];
    if output_width == 0 || output_height == 0 || factor == 0 {
        return output;
    }
    debug_assert_eq!(internal_width, output_width.saturating_mul(factor));

    let workers = threads.clamp(1, output_height as usize);
    if workers <= 1 {
        resolve_rows(
            premultiplied,
            row_stride,
            output_width,
            factor,
            0,
            output_height,
            &mut output,
        );
        return output;
    }

    let row_bytes = output_width as usize * 4;
    let rows_per_chunk = (output_height as usize).div_ceil(workers);
    let chunk_bytes = rows_per_chunk * row_bytes;

    std::thread::scope(|scope| {
        for (chunk_index, chunk) in output.chunks_mut(chunk_bytes).enumerate() {
            let start_y = (chunk_index * rows_per_chunk) as u32;
            let rows = (chunk.len() / row_bytes) as u32;
            scope.spawn(move || {
                resolve_rows(
                    premultiplied,
                    row_stride,
                    output_width,
                    factor,
                    start_y,
                    rows,
                    chunk,
                );
            });
        }
    });

    output
}

#[allow(clippy::too_many_arguments)]
fn resolve_rows(
    premultiplied: &[u8],
    row_stride: usize,
    output_width: u32,
    factor: u32,
    start_y: u32,
    rows: u32,
    out: &mut [u8],
) {
    let count = u64::from(factor) * u64::from(factor);
    let row_bytes = output_width as usize * 4;

    for local_y in 0..rows {
        let output_y = start_y + local_y;
        let out_start = local_y as usize * row_bytes;
        let out_row = &mut out[out_start..out_start + row_bytes];

        for output_x in 0..output_width {
            let mut alpha_sum = 0u64;
            let mut premultiplied_sum = [0u64; 3];

            for sample_y in 0..factor {
                let sample_row = (output_y * factor + sample_y) as usize;
                let row = &premultiplied[sample_row * row_stride..];
                for sample_x in 0..factor {
                    let offset = (output_x * factor + sample_x) as usize * 4;
                    alpha_sum += u64::from(row[offset + 3]);
                    premultiplied_sum[0] += u64::from(row[offset]);
                    premultiplied_sum[1] += u64::from(row[offset + 1]);
                    premultiplied_sum[2] += u64::from(row[offset + 2]);
                }
            }

            let target = output_x as usize * 4;
            let alpha = round_div(alpha_sum, count);
            out_row[target + 3] = alpha as u8;
            if alpha_sum == 0 {
                out_row[target] = 0;
                out_row[target + 1] = 0;
                out_row[target + 2] = 0;
            } else {
                out_row[target] = round_div(premultiplied_sum[0] * 255, alpha_sum) as u8;
                out_row[target + 1] = round_div(premultiplied_sum[1] * 255, alpha_sum) as u8;
                out_row[target + 2] = round_div(premultiplied_sum[2] * 255, alpha_sum) as u8;
            }
        }
    }
}

fn round_div(value: u64, divisor: u64) -> u64 {
    (value + divisor / 2) / divisor
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolves_single_sample_and_straightens_alpha() {
        let input = [100, 50, 25, 128];
        let output = resolve(&input, 4, 1, 1, 1, 1, 1);
        assert_eq!(output, [199, 100, 50, 128]);
    }

    #[test]
    fn averages_premultiplied_samples_and_ignores_row_padding() {
        let row_stride = 12;
        let mut input = vec![0u8; row_stride * 2];
        input[0..4].copy_from_slice(&[10, 20, 30, 255]);
        input[4..8].copy_from_slice(&[30, 40, 50, 255]);
        input[8..12].copy_from_slice(&[200, 200, 200, 200]);
        input[12..16].copy_from_slice(&[50, 100, 150, 255]);
        input[16..20].copy_from_slice(&[150, 200, 250, 255]);
        input[20..24].copy_from_slice(&[1, 2, 3, 4]);

        let output = resolve(&input, row_stride, 2, 1, 1, 2, 1);
        assert_eq!(output, [60, 90, 120, 255]);
    }

    #[test]
    fn fully_transparent_pixels_stay_black() {
        let input = [77, 88, 99, 0];
        assert_eq!(resolve(&input, 4, 1, 1, 1, 1, 1), [0, 0, 0, 0]);
    }
}
