use crate::Mesh;
use std::io::{self, Write};

/// Write the mesh as an OBJ file with page-origin-adjusted vertices.
pub fn write_obj<W: Write>(mesh: &Mesh, writer: &mut W) -> io::Result<()> {
    writeln!(writer, "# litematic mesh")?;

    let mut vertex_index = 1usize;
    let mut normal_index = 1usize;

    for page in &mesh.pages {
        for quad in &page.quads {
            for vertex in &quad.vertices {
                let x = page.origin[0] as f64 + f64::from(vertex.position[0]);
                let y = page.origin[1] as f64 + f64::from(vertex.position[1]);
                let z = page.origin[2] as f64 + f64::from(vertex.position[2]);
                writeln!(writer, "v {x} {y} {z}")?;
            }

            let normal = quad.normal;
            writeln!(writer, "vn {} {} {}", normal[0], normal[1], normal[2])?;
            writeln!(
                writer,
                "f {}//{} {}//{} {}//{} {}//{}",
                vertex_index,
                normal_index,
                vertex_index + 1,
                normal_index,
                vertex_index + 2,
                normal_index,
                vertex_index + 3,
                normal_index
            )?;

            vertex_index += 4;
            normal_index += 1;
        }
    }

    Ok(())
}

/// Write the mesh as an ASCII PLY with RGBA vertex colors.
pub fn write_ply<W: Write>(mesh: &Mesh, writer: &mut W) -> io::Result<()> {
    let vertex_count: usize = mesh.pages.iter().map(|page| page.quads.len() * 4).sum();
    let face_count: usize = mesh.pages.iter().map(|page| page.quads.len()).sum();

    writeln!(writer, "ply")?;
    writeln!(writer, "format ascii 1.0")?;
    writeln!(writer, "comment litematic mesh")?;
    writeln!(writer, "element vertex {vertex_count}")?;
    writeln!(writer, "property double x")?;
    writeln!(writer, "property double y")?;
    writeln!(writer, "property double z")?;
    writeln!(writer, "property uchar red")?;
    writeln!(writer, "property uchar green")?;
    writeln!(writer, "property uchar blue")?;
    writeln!(writer, "property uchar alpha")?;
    writeln!(writer, "element face {face_count}")?;
    writeln!(writer, "property list uchar int vertex_indices")?;
    writeln!(writer, "end_header")?;

    for page in &mesh.pages {
        for quad in &page.quads {
            for vertex in &quad.vertices {
                let x = page.origin[0] as f64 + f64::from(vertex.position[0]);
                let y = page.origin[1] as f64 + f64::from(vertex.position[1]);
                let z = page.origin[2] as f64 + f64::from(vertex.position[2]);
                let [r, g, b, a] = quad.color;
                writeln!(writer, "{x} {y} {z} {r} {g} {b} {a}")?;
            }
        }
    }

    let mut index = 0usize;
    for page in &mesh.pages {
        for _ in &page.quads {
            writeln!(
                writer,
                "4 {} {} {} {}",
                index,
                index + 1,
                index + 2,
                index + 3
            )?;
            index += 4;
        }
    }

    Ok(())
}
