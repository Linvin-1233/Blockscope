pub(super) fn downsample(
    input: &[u8],
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    factor: u32,
    threads: usize,
) -> Vec<u8> {
    let mut output = vec![0u8; (u64::from(output_width) * u64::from(output_height) * 4) as usize];
    if output_width == 0 || output_height == 0 || factor == 0 {
        return output;
    }

    let workers = threads.clamp(1, output_height as usize);
    if workers <= 1 {
        downsample_rows(
            input,
            input_width,
            input_height,
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
                downsample_rows(
                    input,
                    input_width,
                    input_height,
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
fn downsample_rows(
    input: &[u8],
    input_width: u32,
    input_height: u32,
    output_width: u32,
    factor: u32,
    start_y: u32,
    rows: u32,
    out: &mut [u8],
) {
    let factor_f = f64::from(factor);
    let count = factor_f * factor_f;
    let row_bytes = output_width as usize * 4;

    for local_y in 0..rows {
        let output_y = start_y + local_y;
        let row_start = local_y as usize * row_bytes;
        let out_row = &mut out[row_start..row_start + row_bytes];

        for output_x in 0..output_width {
            let mut alpha_sum = 0.0f64;
            let mut color_sum = [0.0f64; 3];

            for sample_y in 0..factor {
                for sample_x in 0..factor {
                    let input_x = output_x * factor + sample_x;
                    let input_y = output_y * factor + sample_y;
                    if input_x >= input_width || input_y >= input_height {
                        continue;
                    }
                    let index = (u64::from(input_y) * u64::from(input_width) + u64::from(input_x))
                        as usize
                        * 4;
                    let alpha = f64::from(input[index + 3]) / 255.0;
                    alpha_sum += alpha;
                    color_sum[0] += f64::from(input[index]) / 255.0 * alpha;
                    color_sum[1] += f64::from(input[index + 1]) / 255.0 * alpha;
                    color_sum[2] += f64::from(input[index + 2]) / 255.0 * alpha;
                }
            }

            let average_alpha = alpha_sum / count;
            let output_index = output_x as usize * 4;
            out_row[output_index + 3] = (average_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha_sum > 0.0 {
                out_row[output_index] =
                    ((color_sum[0] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
                out_row[output_index + 1] =
                    ((color_sum[1] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
                out_row[output_index + 2] =
                    ((color_sum[2] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_parallel_matches_single_thread() {
        let input_width = 32;
        let input_height = 32;
        let factor = 2;
        let output_width = 16;
        let output_height = 16;
        let mut input = vec![0u8; (input_width * input_height * 4) as usize];
        for (i, byte) in input.iter_mut().enumerate() {
            *byte = ((i * 7 + 13) % 256) as u8;
        }

        let serial = downsample(
            &input,
            input_width,
            input_height,
            output_width,
            output_height,
            factor,
            1,
        );

        for threads in [2, 3, 4, 8, 16] {
            let parallel = downsample(
                &input,
                input_width,
                input_height,
                output_width,
                output_height,
                factor,
                threads,
            );
            assert_eq!(serial, parallel, "threads {threads} must match serial");
        }
    }
}
