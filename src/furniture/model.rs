use glam::{Mat3, Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// A triangle mesh loaded from a glTF/GLB file, in meters.
/// It is recentered so the footprint is centered on the origin and it rests on y = 0.
pub struct ModelMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Extent along x, y (up) and z.
    pub size: [f32; 3],
}

/// Caches loaded models by path, including failures so a broken file isn't re-read every frame.
#[derive(Default)]
pub struct ModelLibrary {
    cache: HashMap<String, Result<Arc<ModelMesh>, String>>,
}

impl ModelLibrary {
    pub fn get(&mut self, path: &str) -> Result<Arc<ModelMesh>, String> {
        self.cache
            .entry(path.to_owned())
            .or_insert_with(|| load(path).map(Arc::new))
            .clone()
    }
}

fn load(path: &str) -> Result<ModelMesh, String> {
    let (doc, buffers, images) =
        gltf::import(path).map_err(|e| format!("Could not load {path}: {e}"))?;

    let mut mesh = ModelMesh {
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
        size: [0.0; 3],
    };

    let scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .ok_or_else(|| format!("{path} contains no scene"))?;

    // Average color of each texture, used to tint untextured rendering.
    let mut texture_colors: HashMap<usize, [f32; 3]> = HashMap::new();
    for node in scene.nodes() {
        add_node(
            &node,
            Mat4::IDENTITY,
            &buffers,
            &images,
            &mut texture_colors,
            &mut mesh,
        );
    }

    if mesh.indices.is_empty() {
        return Err(format!("{path} contains no triangles"));
    }

    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for p in &mesh.positions {
        min = min.min(Vec3::from(*p));
        max = max.max(Vec3::from(*p));
    }
    let offset = Vec3::new((min.x + max.x) / 2.0, min.y, (min.z + max.z) / 2.0);
    for p in &mut mesh.positions {
        *p = (Vec3::from(*p) - offset).to_array();
    }
    mesh.size = (max - min).to_array();

    Ok(mesh)
}

fn add_node(
    node: &gltf::Node,
    parent: Mat4,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    texture_colors: &mut HashMap<usize, [f32; 3]>,
    mesh: &mut ModelMesh,
) {
    let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();

    if let Some(node_mesh) = node.mesh() {
        for primitive in node_mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let Some(positions) = reader.read_positions() else {
                continue;
            };

            let base = mesh.positions.len() as u32;
            let positions: Vec<Vec3> = positions
                .map(|p| transform.transform_point3(Vec3::from(p)))
                .collect();
            let count = positions.len() as u32;
            let indices: Vec<u32> = match reader.read_indices() {
                Some(indices) => indices.into_u32().collect(),
                None => (0..count).collect(),
            };

            let normals: Vec<Vec3> = match reader.read_normals() {
                Some(normals) => normals
                    .map(|n| (normal_matrix * Vec3::from(n)).normalize_or_zero())
                    .collect(),
                None => smooth_normals(&positions, &indices),
            };

            let color = material_color(&primitive.material(), images, texture_colors);

            mesh.positions
                .extend(positions.iter().map(|p| p.to_array()));
            mesh.normals.extend(normals.iter().map(|n| n.to_array()));
            mesh.colors
                .extend(std::iter::repeat_n(color, count as usize));
            mesh.indices.extend(indices.iter().map(|i| base + i));
        }
    }

    for child in node.children() {
        add_node(&child, transform, buffers, images, texture_colors, mesh);
    }
}

fn smooth_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        normals[a] += face;
        normals[b] += face;
        normals[c] += face;
    }
    normals.into_iter().map(Vec3::normalize_or_zero).collect()
}

/// A single color per material in sRGB space (the 3D view has no texture support yet),
/// combining the base color factor with the average color of the base color texture.
fn material_color(
    material: &gltf::Material,
    images: &[gltf::image::Data],
    texture_colors: &mut HashMap<usize, [f32; 3]>,
) -> [f32; 3] {
    let pbr = material.pbr_metallic_roughness();
    let factor = pbr.base_color_factor();
    let mut color = [0, 1, 2].map(|i| factor[i].max(0.0).powf(1.0 / 2.2));

    if let Some(info) = pbr.base_color_texture() {
        let image_index = info.texture().source().index();
        let tex = *texture_colors
            .entry(image_index)
            .or_insert_with(|| images.get(image_index).map_or([1.0; 3], average_color));
        color = [0, 1, 2].map(|i| color[i] * tex[i]);
    }
    color
}

fn average_color(image: &gltf::image::Data) -> [f32; 3] {
    use gltf::image::Format;
    let channels = match image.format {
        Format::R8G8B8 => 3,
        Format::R8G8B8A8 => 4,
        _ => return [1.0; 3],
    };
    let pixel_count = image.pixels.len() / channels;
    if pixel_count == 0 {
        return [1.0; 3];
    }
    // Sampling ~10k pixels is plenty for an average and keeps big textures fast.
    let step = (pixel_count / 10_000).max(1);
    let mut sum = [0.0_f64; 3];
    let mut samples = 0.0;
    for pixel in image.pixels.chunks_exact(channels).step_by(step) {
        for i in 0..3 {
            sum[i] += pixel[i] as f64;
        }
        samples += 1.0;
    }
    sum.map(|s| (s / samples / 255.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single triangle, translated by its node, with a red material and no normals.
    const TRIANGLE_GLTF: &str = r#"{
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "translation": [5.0, 1.0, 0.0]}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                       "min": [0.0, 0.0, 0.0], "max": [2.0, 1.0, 0.0]}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}],
        "buffers": [{"byteLength": 36, "uri": "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAgD8AAAAA"}]
    }"#;

    #[test]
    fn loads_and_recenters_gltf() {
        let dir = std::env::temp_dir().join(format!("roomplanner-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("triangle.gltf");
        std::fs::write(&path, TRIANGLE_GLTF).unwrap();

        let mesh = load(path.to_str().unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(mesh.indices, vec![0, 1, 2]);
        assert_eq!(mesh.size, [2.0, 1.0, 0.0]);
        // Node translation is applied, then the footprint is centered and the base put on y = 0.
        assert_eq!(
            mesh.positions,
            vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [-1.0, 1.0, 0.0]]
        );
        assert_eq!(mesh.colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn missing_file_is_an_error() {
        let mut library = ModelLibrary::default();
        assert!(library.get("/nonexistent/model.glb").is_err());
    }
}
