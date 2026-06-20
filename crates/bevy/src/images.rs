//! Image loading for the `Image` component — resolve an image URL, decode the
//! raster bytes into a Bevy `Image` asset, and cache the `Handle`s on
//! [`crate::state::A2uiState`].
//!
//! Source resolution (`http(s)` / `data:` / `file://` / local path) and raster
//! decode are shared with every other backend via the `a2ui-image` crate; this
//! module only wraps the resulting RGBA into a Bevy `Image`. Unlike Iced (which
//! fetches asynchronously on its thread-pool executor) Bevy's gallery has no
//! convenient async hook into the reconciler's per-frame `Handle` lookup, so
//! decoding runs **synchronously on the UI thread** in [`load_images`] —
//! matching the documented Slint behavior. The gallery samples carry only a
//! handful of small images, so the one-time per-URL cost is acceptable. Results
//! are cached by resolved URL and cleared on sample switch.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use a2ui_base::model::component_context::ComponentContext;
use a2ui_base::protocol::common_types::DynamicString;

use crate::state::A2uiState;

/// Decode/fetch every `Image` component's URL into the cache (once per URL).
/// Runs first in the render-loop chain so the reconciler (same frame) can pick
/// up freshly-decoded handles. Idempotent: URLs already in the cache (including
/// `None` for prior failures) are skipped, so this is cheap after the first
/// frame a sample is shown.
///
/// Split into a read pass (collect uncached URLs while borrowing the model) and
/// a write pass (decode + insert into the cache) so the model's `Ref` is dropped
/// before the cache is mutated — the same collected-then-applied shape the rest
/// of the backend uses.
pub fn load_images(mut state: NonSendMut<A2uiState>, mut assets: ResMut<Assets<Image>>) {
    // Read pass: collect Image URLs not yet in the cache.
    let urls: Vec<String> = {
        let Some(surface) = state.processor.model.surfaces().next() else {
            return;
        };
        let components = surface.components.borrow();
        let data_model = surface.data_model.borrow();
        components
            .all()
            .iter()
            .filter_map(|(id, model)| {
                if model.component_type != "Image" {
                    return None;
                }
                let ctx = ComponentContext::new(
                    id.clone(),
                    String::new(),
                    &data_model,
                    &components,
                    &state.functions,
                    "",
                    None,
                );
                let url = model
                    .get_property::<DynamicString>("url")
                    .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
                    .unwrap_or_default();
                if url.is_empty() || state.image_cache.contains_key(&url) {
                    return None;
                }
                Some(url)
            })
            .collect()
    };

    // Write pass: decode each URL and cache the handle (or None on failure).
    for url in urls {
        let handle = load_image_sync(&url, &mut assets);
        state.image_cache.insert(url, handle);
    }
}

/// Resolve + decode one image URL to a Bevy `Handle<Image>` (or `None` on any
/// failure). Source resolution and raster decode are delegated to `a2ui-image`;
/// this just maps the resulting RGBA into a Bevy `Image` asset. Blocking — see
/// the module docs.
fn load_image_sync(url: &str, assets: &mut Assets<Image>) -> Option<Handle<Image>> {
    let bytes = a2ui_image::resolve_bytes(url)?;
    let img = a2ui_image::decode(&bytes)?;
    let bevy_img = Image::new(
        Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        img.rgba,
        // `image`'s `to_rgba8()` yields sRGB-encoded RGBA; interpret it as such.
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    Some(assets.add(bevy_img))
}
