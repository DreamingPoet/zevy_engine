use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use bevy::{
    asset::{AssetApp, AssetId, AssetLoader, LoadContext, LoadState, io::Reader},
    image::ImageFilterMode,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{TextureDimension, TextureFormat},
};
use thiserror::Error;

const ZEVY_MIP_MAGIC: &[u8; 8] = b"ZEVYMIP\0";
const ZEVY_MIP_VERSION: u32 = 1;
const ZEVY_MIP_HEADER_SIZE: usize = 32;
const ZEVY_MIP_FLAG_DEBUG_NUMBERS: u32 = 1;
const EXPORTED_TEXTURE_ASSET_ROOT: &str = "levels/";
const EXPORTED_TEXTURE_ANISOTROPY: u16 = 16;
const EXPORTED_TEXTURE_MIN_LOD: f32 = 0.0;

#[derive(Asset, TypePath, Debug)]
pub struct ZevyMipTexture {
    pub width: u32,
    pub height: u32,
    pub mip_level_count: u32,
    pub debug_numbers: bool,
    pub rgba8_data: Vec<u8>,
}

#[derive(Default)]
pub struct ZevyMipTextureLoader;

#[derive(Debug, Error)]
pub enum ZevyMipTextureLoaderError {
    #[error("could not read Zevy mip texture: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Zevy mip texture: {0}")]
    Invalid(String),
    #[error("could not decode a Zevy mip PNG: {0}")]
    Png(#[from] image::ImageError),
}

impl AssetLoader for ZevyMipTextureLoader {
    type Asset = ZevyMipTexture;
    type Settings = ();
    type Error = ZevyMipTextureLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        parse_zevy_mip_texture(&bytes)
    }

    fn extensions(&self) -> &[&str] {
        &["zevy-mips"]
    }
}

pub struct ZevyMipTexturePlugin;

impl Plugin for ZevyMipTexturePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ZevyMipTexture>()
            .init_asset_loader::<ZevyMipTextureLoader>()
            .add_systems(Update, apply_exported_mip_sidecars);
    }
}

#[derive(Clone)]
struct PendingMipSidecar {
    texture_path: String,
    sidecar_path: String,
    handle: Handle<ZevyMipTexture>,
}

#[derive(Default)]
struct ExportedMipState {
    pending: HashMap<AssetId<Image>, PendingMipSidecar>,
    applied: HashSet<AssetId<Image>>,
    failed: HashSet<AssetId<Image>>,
}

fn apply_exported_mip_sidecars(
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mip_textures: ResMut<Assets<ZevyMipTexture>>,
    mut state: Local<ExportedMipState>,
) {
    let pending_ids = state.pending.keys().copied().collect::<Vec<_>>();
    for image_id in pending_ids {
        let Some(pending) = state.pending.get(&image_id).cloned() else {
            continue;
        };

        if let Some(mip_texture) = mip_textures.remove(pending.handle.id()) {
            let result = images
                .get_mut(image_id)
                .ok_or_else(|| "base image was unloaded before its mip sidecar".to_owned())
                .and_then(|image| apply_mip_texture(image, mip_texture));
            state.pending.remove(&image_id);
            match result {
                Ok((mip_level_count, debug_numbers)) => {
                    let refreshed_materials =
                        refresh_standard_materials_for_image(&mut materials, image_id);
                    state.applied.insert(image_id);
                    info!(
                        "Applied exported {}-level mip chain to {} from {} (debug numbers: {}, refreshed {} material bindings, {} textures completed)",
                        mip_level_count,
                        pending.texture_path,
                        pending.sidecar_path,
                        debug_numbers,
                        refreshed_materials,
                        state.applied.len(),
                    );
                }
                Err(error) => {
                    state.failed.insert(image_id);
                    warn!(
                        "Unable to apply exported mip chain {} to {}: {}",
                        pending.sidecar_path, pending.texture_path, error,
                    );
                }
            }
        } else if let LoadState::Failed(error) = asset_server.load_state(pending.handle.id()) {
            state.pending.remove(&image_id);
            state.failed.insert(image_id);
            warn!(
                "Unable to load exported mip sidecar {} for {}: {}",
                pending.sidecar_path, pending.texture_path, error,
            );
        }
    }

    if !state.pending.is_empty() {
        return;
    }

    let texture_root = EXPORTED_TEXTURE_ASSET_ROOT.to_ascii_lowercase();
    let next_texture = images.iter().find_map(|(image_id, image)| {
        if state.applied.contains(&image_id)
            || state.failed.contains(&image_id)
            || image.texture_descriptor.mip_level_count > 1
        {
            return None;
        }

        let asset_path = asset_server.get_path(image_id)?;
        let texture_path = asset_path.path().to_string_lossy().replace('\\', "/");
        if !texture_path.to_ascii_lowercase().starts_with(&texture_root) {
            return None;
        }

        let mut sidecar_path = PathBuf::from(&texture_path);
        sidecar_path.set_extension("zevy-mips");
        Some((
            image_id,
            texture_path,
            sidecar_path.to_string_lossy().replace('\\', "/"),
        ))
    });

    if let Some((image_id, texture_path, sidecar_path)) = next_texture {
        let handle = asset_server.load::<ZevyMipTexture>(sidecar_path.clone());
        state.pending.insert(
            image_id,
            PendingMipSidecar {
                texture_path,
                sidecar_path,
                handle,
            },
        );
    }
}

fn refresh_standard_materials_for_image(
    materials: &mut Assets<StandardMaterial>,
    image_id: AssetId<Image>,
) -> usize {
    let material_ids = materials
        .iter()
        .filter_map(|(material_id, material)| {
            standard_material_uses_image(material, image_id).then_some(material_id)
        })
        .collect::<Vec<_>>();

    for material_id in &material_ids {
        // Assets::get_mut emits AssetEvent::Modified. This forces Bevy to rebuild the
        // material bind group with the replacement GpuImage texture view and sampler.
        let _ = materials.get_mut(*material_id);
    }
    material_ids.len()
}

fn standard_material_uses_image(material: &StandardMaterial, image_id: AssetId<Image>) -> bool {
    [
        &material.base_color_texture,
        &material.emissive_texture,
        &material.metallic_roughness_texture,
        &material.normal_map_texture,
        &material.occlusion_texture,
    ]
    .into_iter()
    .flatten()
    .any(|texture| texture.id() == image_id)
}

fn apply_mip_texture(
    image: &mut Image,
    mip_texture: ZevyMipTexture,
) -> Result<(u32, bool), String> {
    if image.texture_descriptor.dimension != TextureDimension::D2
        || image.texture_descriptor.size.depth_or_array_layers != 1
    {
        return Err("base image is not a non-array 2D texture".to_owned());
    }
    if image.texture_descriptor.size.width != mip_texture.width
        || image.texture_descriptor.size.height != mip_texture.height
    {
        return Err(format!(
            "base image is {}x{}, but sidecar is {}x{}",
            image.texture_descriptor.size.width,
            image.texture_descriptor.size.height,
            mip_texture.width,
            mip_texture.height,
        ));
    }
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) {
        return Err(format!(
            "base image format {:?} is not RGBA8",
            image.texture_descriptor.format,
        ));
    }

    let mip_level_count = mip_texture.mip_level_count;
    let debug_numbers = mip_texture.debug_numbers;
    image.data = Some(mip_texture.rgba8_data);
    image.texture_descriptor.mip_level_count = mip_level_count;
    let sampler = image.sampler.get_or_init_descriptor();
    sampler.mag_filter = ImageFilterMode::Linear;
    sampler.min_filter = ImageFilterMode::Linear;
    sampler.mipmap_filter = ImageFilterMode::Linear;
    sampler.lod_min_clamp = EXPORTED_TEXTURE_MIN_LOD.min((mip_level_count - 1) as f32);
    sampler.lod_max_clamp = (mip_level_count - 1) as f32;
    sampler.anisotropy_clamp = EXPORTED_TEXTURE_ANISOTROPY;

    Ok((mip_level_count, debug_numbers))
}

fn parse_zevy_mip_texture(bytes: &[u8]) -> Result<ZevyMipTexture, ZevyMipTextureLoaderError> {
    if bytes.len() < ZEVY_MIP_HEADER_SIZE {
        return Err(ZevyMipTextureLoaderError::Invalid(
            "file is shorter than the 32-byte header".to_owned(),
        ));
    }
    if &bytes[..8] != ZEVY_MIP_MAGIC {
        return Err(ZevyMipTextureLoaderError::Invalid(
            "magic must be ZEVYMIP\\0".to_owned(),
        ));
    }

    let mut cursor = 8;
    let version = read_u32(bytes, &mut cursor)?;
    if version != ZEVY_MIP_VERSION {
        return Err(ZevyMipTextureLoaderError::Invalid(format!(
            "unsupported version {version}; expected {ZEVY_MIP_VERSION}",
        )));
    }
    let flags = read_u32(bytes, &mut cursor)?;
    let width = read_u32(bytes, &mut cursor)?;
    let height = read_u32(bytes, &mut cursor)?;
    let mip_level_count = read_u32(bytes, &mut cursor)?;
    let _reserved = read_u32(bytes, &mut cursor)?;
    if width == 0 || height == 0 {
        return Err(ZevyMipTextureLoaderError::Invalid(
            "base dimensions must be non-zero".to_owned(),
        ));
    }
    let expected_mip_level_count = u32::BITS - width.max(height).leading_zeros();
    if mip_level_count != expected_mip_level_count {
        return Err(ZevyMipTextureLoaderError::Invalid(format!(
            "mip level count {mip_level_count} is not a complete chain; expected {expected_mip_level_count}",
        )));
    }

    let mut expected_width = width;
    let mut expected_height = height;
    let mut rgba8_data = Vec::with_capacity(rgba8_mip_chain_byte_len(width, height));
    for level in 0..mip_level_count {
        let level_width = read_u32(bytes, &mut cursor)?;
        let level_height = read_u32(bytes, &mut cursor)?;
        let png_byte_len = read_u32(bytes, &mut cursor)? as usize;
        if level_width != expected_width || level_height != expected_height {
            return Err(ZevyMipTextureLoaderError::Invalid(format!(
                "mip {level} is {level_width}x{level_height}; expected {expected_width}x{expected_height}",
            )));
        }
        let png_end = cursor.checked_add(png_byte_len).ok_or_else(|| {
            ZevyMipTextureLoaderError::Invalid(format!("mip {level} PNG size overflows"))
        })?;
        let png_bytes = bytes.get(cursor..png_end).ok_or_else(|| {
            ZevyMipTextureLoaderError::Invalid(format!("mip {level} PNG is truncated"))
        })?;
        cursor = png_end;

        let decoded =
            image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)?.into_rgba8();
        if decoded.width() != level_width || decoded.height() != level_height {
            return Err(ZevyMipTextureLoaderError::Invalid(format!(
                "decoded mip {level} is {}x{}; expected {level_width}x{level_height}",
                decoded.width(),
                decoded.height(),
            )));
        }
        rgba8_data.extend_from_slice(decoded.as_raw());
        expected_width = (expected_width / 2).max(1);
        expected_height = (expected_height / 2).max(1);
    }

    if cursor != bytes.len() {
        return Err(ZevyMipTextureLoaderError::Invalid(format!(
            "{} trailing bytes remain after the mip chain",
            bytes.len() - cursor,
        )));
    }

    Ok(ZevyMipTexture {
        width,
        height,
        mip_level_count,
        debug_numbers: flags & ZEVY_MIP_FLAG_DEBUG_NUMBERS != 0,
        rgba8_data,
    })
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, ZevyMipTextureLoaderError> {
    let end = cursor
        .checked_add(4)
        .ok_or_else(|| ZevyMipTextureLoaderError::Invalid("integer offset overflows".to_owned()))?;
    let raw = bytes
        .get(*cursor..end)
        .ok_or_else(|| ZevyMipTextureLoaderError::Invalid("unexpected end of file".to_owned()))?;
    *cursor = end;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}

fn rgba8_mip_chain_byte_len(mut width: u32, mut height: u32) -> usize {
    let mut byte_len = 0;
    loop {
        byte_len += width as usize * height as usize * 4;
        if width == 1 && height == 1 {
            return byte_len;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn test_png(width: u32, height: u32, value: u8) -> Vec<u8> {
        let image =
            image::RgbaImage::from_pixel(width, height, image::Rgba([value, value, value, 255]));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        cursor.into_inner()
    }

    fn test_sidecar(debug_numbers: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZEVY_MIP_MAGIC);
        append_u32(&mut bytes, ZEVY_MIP_VERSION);
        append_u32(
            &mut bytes,
            if debug_numbers {
                ZEVY_MIP_FLAG_DEBUG_NUMBERS
            } else {
                0
            },
        );
        append_u32(&mut bytes, 2);
        append_u32(&mut bytes, 2);
        append_u32(&mut bytes, 2);
        append_u32(&mut bytes, 0);
        for (width, height, value) in [(2, 2, 32), (1, 1, 192)] {
            let png = test_png(width, height, value);
            append_u32(&mut bytes, width);
            append_u32(&mut bytes, height);
            append_u32(&mut bytes, png.len() as u32);
            bytes.extend_from_slice(&png);
        }
        bytes
    }

    #[test]
    fn parses_complete_png_mip_chain() {
        let parsed = parse_zevy_mip_texture(&test_sidecar(true)).unwrap();
        assert_eq!(parsed.width, 2);
        assert_eq!(parsed.height, 2);
        assert_eq!(parsed.mip_level_count, 2);
        assert!(parsed.debug_numbers);
        assert_eq!(parsed.rgba8_data.len(), 20);
        assert_eq!(&parsed.rgba8_data[16..20], &[192, 192, 192, 255]);
    }

    #[test]
    fn rejects_incomplete_mip_chain() {
        let mut bytes = test_sidecar(false);
        bytes[24..28].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            parse_zevy_mip_texture(&bytes),
            Err(ZevyMipTextureLoaderError::Invalid(message))
                if message.contains("complete chain")
        ));
    }

    #[test]
    fn applies_complete_chain_and_trilinear_anisotropic_sampler() {
        let mip_texture = parse_zevy_mip_texture(&test_sidecar(true)).unwrap();
        let mut image = Image::default();
        image.texture_descriptor.size.width = 2;
        image.texture_descriptor.size.height = 2;
        image.texture_descriptor.size.depth_or_array_layers = 1;
        image.texture_descriptor.dimension = TextureDimension::D2;
        image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;

        let (mip_level_count, debug_numbers) = apply_mip_texture(&mut image, mip_texture).unwrap();

        assert_eq!(mip_level_count, 2);
        assert!(debug_numbers);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(image.data.as_ref().unwrap().len(), 20);
        let bevy::image::ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("exported mip texture must use an explicit sampler");
        };
        assert!(matches!(sampler.min_filter, ImageFilterMode::Linear));
        assert!(matches!(sampler.mag_filter, ImageFilterMode::Linear));
        assert!(matches!(sampler.mipmap_filter, ImageFilterMode::Linear));
        assert_eq!(sampler.lod_min_clamp, 0.0);
        assert_eq!(sampler.lod_max_clamp, 1.0);
        assert_eq!(sampler.anisotropy_clamp, EXPORTED_TEXTURE_ANISOTROPY);
    }

    #[test]
    fn finds_and_refreshes_materials_that_reference_replaced_images() {
        let matching_image = Handle::<Image>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let matching_material = materials.add(StandardMaterial {
            base_color_texture: Some(matching_image.clone()),
            ..default()
        });
        materials.add(StandardMaterial::default());

        assert!(standard_material_uses_image(
            materials.get(matching_material.id()).unwrap(),
            matching_image.id(),
        ));
        assert_eq!(
            refresh_standard_materials_for_image(&mut materials, matching_image.id()),
            1,
        );
    }
}
