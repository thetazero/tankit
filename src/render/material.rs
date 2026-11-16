//! The one place surface materials are defined.
//!
//! A material picks how the scene shader shades a surface (`scene.wgsl`
//! branches on it). Meshes carry it in the vertex color's alpha channel as
//! `Material::id()`. The shader never uses raw numbers: the renderer prepends
//! a `const MAT_<NAME>: i32 = <id>;` line for every variant (see
//! `wgsl_constants`), and shader code compares against those names.
//!
//! To add a material: add a variant at the end of the enum and to `ALL`, give
//! it a name in `name`, and handle `MAT_<NAME>` in `scene.wgsl`. The tests
//! fail if the shader never mentions it. Two people adding materials at once
//! get an ordinary merge conflict here instead of a silent id clash.

/// Surface materials, by how the shader treats them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Material {
    // Flat-shaded world (procedural city).
    /// No surface detail.
    Plain,
    /// Building wall: window grid, a few lit windows.
    Wall,
    Asphalt,
    Roof,
    Grass,
    /// Paving tiles.
    Paving,

    // Tanks (object-space detail, moves with the part).
    /// Dark composite with a fine weave.
    Carbon,
    /// Light armor paint; multiplied by the team color.
    Paint,
    Tread,
    /// Pulsing emissive light strip.
    Glow,
    Metal,
    /// Paint on chamfered edges, worn through to metal in places.
    Worn,

    // World with procedural surface textures (`world_surface`).
    /// Wind-rippled desert sand.
    Sand,
    /// Sun-bleached plaster over masonry: chips, stains, grime streaks.
    Plaster,
    /// Bare cast concrete: blotchy, pitted.
    Concrete,
    /// Broken masonry and stone.
    Rubble,
    /// Rusted steel.
    Rust,
    /// Weathered paint over steel, rusting through in patches.
    Painted,

    // Tanks again.
    /// Ithessar worked crystal: dark, glassy, flecked deep inside.
    Vhaal,
    /// Sun-faded fabric (awnings, stall roofs).
    Cloth,
    /// Dry, sun-cracked timber (poles, palms, stalls, crates).
    Wood,
}

impl Material {
    /// Every material, in id order.
    pub const ALL: &[Material] = &[
        Material::Plain,
        Material::Wall,
        Material::Asphalt,
        Material::Roof,
        Material::Grass,
        Material::Paving,
        Material::Carbon,
        Material::Paint,
        Material::Tread,
        Material::Glow,
        Material::Metal,
        Material::Worn,
        Material::Sand,
        Material::Plaster,
        Material::Concrete,
        Material::Rubble,
        Material::Rust,
        Material::Painted,
        Material::Vhaal,
        Material::Cloth,
        Material::Wood,
    ];

    /// What goes in the vertex color's alpha channel.
    pub const fn id(self) -> f32 {
        self as u32 as f32
    }

    /// Shader-side name, without the `MAT_` prefix.
    pub const fn name(self) -> &'static str {
        match self {
            Material::Plain => "PLAIN",
            Material::Wall => "WALL",
            Material::Asphalt => "ASPHALT",
            Material::Roof => "ROOF",
            Material::Grass => "GRASS",
            Material::Paving => "PAVING",
            Material::Carbon => "CARBON",
            Material::Paint => "PAINT",
            Material::Tread => "TREAD",
            Material::Glow => "GLOW",
            Material::Metal => "METAL",
            Material::Worn => "WORN",
            Material::Sand => "SAND",
            Material::Plaster => "PLASTER",
            Material::Concrete => "CONCRETE",
            Material::Rubble => "RUBBLE",
            Material::Rust => "RUST",
            Material::Painted => "PAINTED",
            Material::Vhaal => "VHAAL",
            Material::Cloth => "CLOTH",
            Material::Wood => "WOOD",
        }
    }

    /// WGSL declarations for every material, prepended to shaders that use them.
    pub fn wgsl_constants() -> String {
        let mut s = String::from("// Generated from render/material.rs. Do not edit.\n");
        for m in Material::ALL {
            s += &format!("const MAT_{}: i32 = {};\n", m.name(), *m as u32);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_dense_and_names_unique() {
        for (i, m) in Material::ALL.iter().enumerate() {
            assert_eq!(*m as u32, i as u32, "{m:?} out of order in ALL");
        }
        let mut names: Vec<&str> = Material::ALL.iter().map(|m| m.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Material::ALL.len(), "duplicate material names");
    }

    /// Every material is handled somewhere in the scene shader (Plain is the
    /// fallback, so it needn't be named).
    #[test]
    fn shader_handles_every_material() {
        let src = include_str!("../shaders/scene.wgsl");
        for m in Material::ALL.iter().filter(|m| **m != Material::Plain) {
            assert!(
                src.contains(&format!("MAT_{}", m.name())),
                "scene.wgsl never handles MAT_{}",
                m.name()
            );
        }
    }

    /// The shader compares against names, never raw ids.
    #[test]
    fn shader_has_no_raw_material_ids() {
        let src = include_str!("../shaders/scene.wgsl");
        for pat in ["m == ", "m != ", "m >= ", "m <= ", "m < ", "m > "] {
            for (i, _) in src.match_indices(pat) {
                let next = src[i + pat.len()..].chars().next().unwrap();
                assert!(
                    !next.is_ascii_digit(),
                    "raw material id in scene.wgsl near: {}",
                    &src[i..(i + 20).min(src.len())]
                );
            }
        }
    }
}
