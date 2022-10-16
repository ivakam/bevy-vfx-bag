//! Credits to Ben Cloward, see: https://www.youtube.com/watch?v=HcMFgJas0yg

use bevy::{
    prelude::*,
    reflect::TypeUuid,
    render::{
        mesh::MeshVertexBufferLayout,
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
        },
    },
    sprite::{Material2d, Material2dKey, Material2dPlugin, MaterialMesh2dBundle},
};

use crate::{BevyVfxBagImage, BevyVfxBagRenderLayer, ShouldResize};

/// This plugin allows adding a mask effect to a texture.
pub struct MaskPlugin;

/// This resource controls the parameters of the effect.
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum MaskVariant {
    /// Rounded square type mask.
    ///
    /// One use of this mask is to post-process _other_ effects which might
    /// have artifacts around the edges.
    /// This mask can then attenuate that effect and thus remove the effects of the
    /// artifacts.
    ///
    /// Strength value guidelines for use in [`Mask`]:
    ///
    /// Low end:    3.0 almost loses the square shape.
    /// High end:   100.0 has almost sharp, thin edges.
    Square,

    /// Rounded square type mask, but more oval like a CRT television.
    ///
    /// This effect can be used as a part of a retry-style effect.
    ///
    /// Strength value guidelines for use in [`Mask`]:
    ///
    /// Low end:    3000.0 almost loses the CRT shape.
    /// High end:   500000.0 "inflates" the effect a bit.
    Crt,

    /// Vignette mask.
    ///
    /// This effect can be used to replicate the classic photography
    /// light attenuation seen at the edges of photos.
    ///
    /// Strength value guidelines for use in [`Mask`]:
    ///
    /// Low end:    0.10 gives a very subtle effect.
    /// High end:   1.50 is almost a spotlight in the middle of the screen.
    Vignette,
}

/// This resource controls the parameters of the effect.
#[derive(Debug, Resource, Clone)]
pub struct Mask {
    /// The strength parameter of the mask in use.
    ///
    /// See [`MaskVariant`] for guidelines on which range of values make sense
    /// for the variant in use.
    ///
    /// Run the masks example to experiment with these values interactively.
    pub strength: f32,

    /// Which [`MaskVariant`] to produce.
    pub variant: MaskVariant,
}

impl Mask {
    /// Create a new square mask with a reasonable strength value.
    pub fn new_square() -> Self {
        Self {
            strength: 20.,
            variant: MaskVariant::Square,
        }
    }

    /// Create a new CRT mask with a reasonable strength value.
    pub fn new_crt() -> Self {
        Self {
            strength: 80000.,
            variant: MaskVariant::Crt,
        }
    }

    /// Create a new vignette mask with a reasonable strength value.
    pub fn new_vignette() -> Self {
        Self {
            strength: 0.66,
            variant: MaskVariant::Vignette,
        }
    }
}

impl From<&MaskMaterial> for MaskVariant {
    fn from(mask_material: &MaskMaterial) -> Self {
        mask_material.variant
    }
}

#[derive(AsBindGroup, TypeUuid, Clone)]
#[uuid = "9ca04144-a3e1-40b4-93a7-91424159f612"]
#[bind_group_data(MaskVariant)]
struct MaskMaterial {
    #[texture(0)]
    #[sampler(1)]
    source_image: Handle<Image>,

    #[uniform(2)]
    strength: f32,

    variant: MaskVariant,
}

impl Material2d for MaskMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/masks.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayout,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let def = match key.bind_group_data {
            MaskVariant::Square => "SQUARE",
            MaskVariant::Crt => "CRT",
            MaskVariant::Vignette => "VIGNETTE",
        };
        descriptor
            .fragment
            .as_mut()
            .expect("Should have fragment state")
            .shader_defs
            .push(def.into());

        Ok(())
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mask_materials: ResMut<Assets<MaskMaterial>>,
    image_handle: Res<BevyVfxBagImage>,
    render_layer: Res<BevyVfxBagRenderLayer>,
    mask: Res<Mask>,
    images: Res<Assets<Image>>,
) {
    let image = images
        .get(&*image_handle)
        .expect("BevyVfxBagImage should exist");
    let extent = image.texture_descriptor.size;

    let quad_handle = meshes.add(Mesh::from(shape::Quad::new(Vec2::new(
        extent.width as f32,
        extent.height as f32,
    ))));

    let material_handle = mask_materials.add(MaskMaterial {
        source_image: image_handle.clone(),
        strength: mask.strength,
        variant: mask.variant,
    });

    // Post processing 2d quad, with material using the render texture done by the main camera, with a custom shader.
    commands.spawn((
        MaterialMesh2dBundle {
            mesh: quad_handle.into(),
            material: material_handle,
            transform: Transform {
                translation: Vec3::new(0.0, 0.0, 1.5),
                ..default()
            },
            ..default()
        },
        render_layer.0,
        ShouldResize,
    ));
}

fn update_mask(mut mask_materials: ResMut<Assets<MaskMaterial>>, mask: Res<Mask>) {
    if !mask.is_changed() {
        return;
    }

    for (_, material) in mask_materials.iter_mut() {
        material.variant = mask.variant;
        material.strength = mask.strength;
    }
}

impl Plugin for MaskPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugin(Material2dPlugin::<MaskMaterial>::default())
            .add_startup_system(setup)
            .add_system(update_mask);
    }
}
