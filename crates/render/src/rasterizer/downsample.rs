pub(super) fn downsample(
    input: &[u8],
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    factor: u32,
) -> Vec<u8> {
    let mut output = vec![0u8; (u64::from(output_width) * u64::from(output_height) * 4) as usize];
    let factor = f64::from(factor);
    let count = factor * factor;

    for output_y in 0..output_height {
        for output_x in 0..output_width {
            let mut alpha_sum = 0.0f64;
            let mut color_sum = [0.0f64; 3];

            for sample_y in 0..factor as u32 {
                for sample_x in 0..factor as u32 {
                    let input_x = output_x * factor as u32 + sample_x;
                    let input_y = output_y * factor as u32 + sample_y;
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
            let output_index =
                (u64::from(output_y) * u64::from(output_width) + u64::from(output_x)) as usize * 4;
            output[output_index + 3] = (average_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha_sum > 0.0 {
                output[output_index] =
                    ((color_sum[0] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
                output[output_index + 1] =
                    ((color_sum[1] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
                output[output_index + 2] =
                    ((color_sum[2] / alpha_sum).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }

    output
}
