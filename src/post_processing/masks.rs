use bevy::{
    asset::load_internal_asset,
    ecs::query::QueryItem,
    prelude::*,
    reflect::TypeUuid,
    render::{
        extract_component::{
            ComponentUniforms, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin,
        },
        render_phase::{AddRenderCommand, DrawFunctions, RenderPhase},
        render_resource::{
            BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry,
            BindingType, BufferBindingType, PipelineCache, RenderPipelineDescriptor, ShaderDefVal,
            ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines,
        },
        renderer::RenderDevice,
        Render, RenderSet,
    },
};
use std::fmt::Display;

use crate::post_processing::DrawPostProcessingEffect;

use super::{Order, PostProcessingPhaseItem, UniformBindGroup};
pub(crate) const MASK_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 1059400090272595510);

#[derive(Resource)]
pub(crate) struct MaskData {
    pub uniform_layout: BindGroupLayout,
    pub shared_layout: BindGroupLayout,
}

impl FromWorld for MaskData {
    fn from_world(world: &mut World) -> Self {
        let uniform_layout = super::create_layout(
            world,
            "Mask",
            &[BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(MaskUniform::min_size()),
                },
                visibility: ShaderStages::FRAGMENT,
                count: None,
            }],
        );

        let shared_layout = world
            .resource::<super::PostProcessingSharedLayout>()
            .shared_layout
            .clone();
        MaskData {
            uniform_layout,
            shared_layout,
        }
    }
}

pub(crate) struct Plugin;
impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            MASK_SHADER_HANDLE,
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/", "masks.wgsl"),
            Shader::from_wgsl
        );

        // This puts the uniform into the render world.
        app.add_plugins((
            ExtractComponentPlugin::<Mask>::default(),
            UniformComponentPlugin::<MaskUniform>::default(),
        ));

        super::render_app(app)
            .add_systems(
                ExtractSchedule,
                super::extract_post_processing_camera_phases::<Mask>,
            )
            .init_resource::<MaskData>()
            .init_resource::<UniformBindGroup<MaskUniform>>()
            .init_resource::<SpecializedRenderPipelines<MaskData>>()
            .add_systems(Render, prepare.in_set(RenderSet::Prepare))
            .add_systems(Render, queue.in_set(RenderSet::Queue))
            .add_render_command::<PostProcessingPhaseItem, DrawPostProcessingEffect<MaskUniform>>();
    }
}

impl SpecializedRenderPipeline for MaskData {
    type Key = MaskVariant;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        super::render_pipeline_descriptor(
            "Masks",
            &self.shared_layout,
            &self.uniform_layout,
            MASK_SHADER_HANDLE.typed(),
            vec![key.into()],
        )
    }
}

fn prepare(
    data: Res<MaskData>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<MaskData>>,
    mut views: Query<(
        Entity,
        &mut RenderPhase<PostProcessingPhaseItem>,
        &Order<Mask>,
        &MaskVariant,
    )>,
    draw_functions: Res<DrawFunctions<PostProcessingPhaseItem>>,
) {
    for (entity, mut phase, order, key) in views.iter_mut() {
        let draw_function = draw_functions
            .read()
            .id::<DrawPostProcessingEffect<MaskUniform>>();

        let pipeline_id = pipelines.specialize(&pipeline_cache, &data, *key);

        phase.add(PostProcessingPhaseItem {
            entity,
            sort_key: (*order).into(),
            draw_function,
            pipeline_id,
        });
    }
}

fn queue(
    render_device: Res<RenderDevice>,
    data: Res<MaskData>,
    mut bind_group: ResMut<UniformBindGroup<MaskUniform>>,
    uniforms: Res<ComponentUniforms<MaskUniform>>,
    views: Query<Entity, With<MaskUniform>>,
) {
    bind_group.inner = None;

    if let Some(uniforms) = uniforms.binding() {
        if !views.is_empty() {
            bind_group.inner = Some(render_device.create_bind_group(&BindGroupDescriptor {
                label: Some("Mask Uniform Bind Group"),
                layout: &data.uniform_layout,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: uniforms.clone(),
                }],
            }));
        }
    }
}

/// This controls the parameters of the effect.
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone, Component)]
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

impl From<MaskVariant> for ShaderDefVal {
    fn from(variant: MaskVariant) -> Self {
        match variant {
            MaskVariant::Square => "SQUARE",
            MaskVariant::Crt => "CRT",
            MaskVariant::Vignette => "VIGNETTE",
        }
        .into()
    }
}

/// A darkening mask on the outer edges of the image.
#[derive(Debug, Copy, Clone, Component)]
pub struct Mask {
    /// The strength parameter of the mask in use.
    ///
    /// See [`MaskVariant`] for guidelines on which range of values make sense
    /// for the variant in use.
    ///
    /// Run the masks example to experiment with these values interactively.
    pub strength: f32,

    /// How much the mask is faded: 1.0 - mask has no effect, 0.0 - mask is in full effect
    pub fade: f32,

    /// Which [`MaskVariant`] to produce.
    pub variant: MaskVariant,
}

impl Display for Mask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mask {:?}, strength: {} fade: {}",
            self.variant, self.strength, self.fade
        )
    }
}

impl Mask {
    /// Create a new square mask with a reasonable strength value.
    pub fn square() -> Self {
        Self {
            strength: 20.,
            fade: 0.,
            variant: MaskVariant::Square,
        }
    }

    /// Create a new CRT mask with a reasonable strength value.
    pub fn crt() -> Self {
        Self {
            strength: 80000.,
            fade: 0.,
            variant: MaskVariant::Crt,
        }
    }

    /// Create a new vignette mask with a reasonable strength value.
    pub fn vignette() -> Self {
        Self {
            strength: 0.66,
            fade: 0.,
            variant: MaskVariant::Vignette,
        }
    }
}

impl Default for Mask {
    fn default() -> Self {
        Self::vignette()
    }
}

#[doc(hidden)]
/// [`Mask`] as a uniform.
#[derive(Debug, ShaderType, Clone, Component, Copy)]
pub struct MaskUniform {
    pub(crate) strength: f32,
    pub(crate) fade: f32,
}

impl From<Mask> for MaskUniform {
    fn from(mask: Mask) -> Self {
        Self {
            strength: mask.strength,
            fade: mask.fade,
        }
    }
}

impl ExtractComponent for Mask {
    type Query = (&'static Self, &'static Camera);
    type Filter = ();
    type Out = (MaskUniform, MaskVariant);

    fn extract_component((settings, camera): QueryItem<'_, Self::Query>) -> Option<Self::Out> {
        if !camera.is_active {
            return None;
        }

        Some(((*settings).into(), settings.variant))
    }
}
