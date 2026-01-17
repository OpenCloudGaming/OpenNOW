//! Image Cache for Game Art
//!
//! Loads and caches game box art images for display in the UI.

use log::{debug, warn};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Image loading state
#[derive(Clone)]
pub enum ImageState {
    /// Not yet requested
    NotLoaded,
    /// Currently loading
    Loading,
    /// Successfully loaded (RGBA pixels, width, height)
    Loaded(Arc<Vec<u8>>, u32, u32),
    /// Failed to load
    Failed,
}

/// Global image cache
pub struct ImageCache {
    /// Map from URL to image state
    images: RwLock<HashMap<String, ImageState>>,
    /// HTTP client for fetching images
    client: reqwest::Client,
}

impl ImageCache {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/128.0.0.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            images: RwLock::new(HashMap::new()),
            client,
        }
    }

    /// Get image state for a URL
    pub fn get(&self, url: &str) -> ImageState {
        let images = self.images.read();
        images.get(url).cloned().unwrap_or(ImageState::NotLoaded)
    }

    /// Request loading an image (non-blocking)
    pub fn request_load(&self, url: String, runtime: tokio::runtime::Handle) {
        // Check if already loading or loaded
        {
            let images = self.images.read();
            if images.contains_key(&url) {
                return; // Already in progress or loaded
            }
        }

        // Mark as loading
        {
            let mut images = self.images.write();
            images.insert(url.clone(), ImageState::Loading);
        }

        // Spawn async task to load image
        let client = self.client.clone();
        let url_clone = url.clone();
        let _images = Arc::new(self.images.read().clone());

        // We need to use a static or leaked reference for the cache update
        // For simplicity, we'll use a channel pattern
        runtime.spawn(async move {
            match Self::load_image_async(&client, &url_clone).await {
                Ok((pixels, width, height)) => {
                    debug!("Loaded image: {} ({}x{})", url_clone, width, height);
                    LOADED_IMAGES.write().insert(
                        url_clone,
                        ImageState::Loaded(Arc::new(pixels), width, height),
                    );
                }
                Err(e) => {
                    warn!("Failed to load image {}: {}", url_clone, e);
                    LOADED_IMAGES.write().insert(url_clone, ImageState::Failed);
                }
            }
        });
    }

    /// Load an image asynchronously
    /// Images are resized to max 300x400 for efficient UI rendering on low-end devices
    async fn load_image_async(
        client: &reqwest::Client,
        url: &str,
    ) -> anyhow::Result<(Vec<u8>, u32, u32)> {
        use anyhow::Context;

        let response = client
            .get(url)
            .header("Accept", "image/webp,image/png,image/jpeg,*/*")
            .header("Referer", "https://play.geforcenow.com/")
            .send()
            .await
            .context("Failed to fetch image")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Image fetch failed: {}", response.status()));
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read image bytes")?;

        // Decode image
        let img = image::load_from_memory(&bytes).context("Failed to decode image")?;

        // Resize to thumbnail size for efficient UI rendering
        // Game cards are typically displayed at ~200x280 pixels
        // Cap at 300x400 to balance quality and performance on low-end devices
        const MAX_WIDTH: u32 = 300;
        const MAX_HEIGHT: u32 = 400;

        let (orig_w, orig_h) = (img.width(), img.height());
        let img = if orig_w > MAX_WIDTH || orig_h > MAX_HEIGHT {
            let scale = (MAX_WIDTH as f32 / orig_w as f32).min(MAX_HEIGHT as f32 / orig_h as f32);
            let new_w = (orig_w as f32 * scale) as u32;
            let new_h = (orig_h as f32 * scale) as u32;
            debug!(
                "Resizing image {}x{} -> {}x{}",
                orig_w, orig_h, new_w, new_h
            );
            img.resize(new_w, new_h, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let pixels = rgba.into_raw();

        Ok((pixels, width, height))
    }

    /// Check for newly loaded images and update cache
    pub fn update(&self) {
        let loaded = LOADED_IMAGES.read().clone();
        if !loaded.is_empty() {
            let mut images = self.images.write();
            for (url, state) in loaded.iter() {
                images.insert(url.clone(), state.clone());
            }
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

// Global storage for loaded images (workaround for async updates)
lazy_static::lazy_static! {
    static ref LOADED_IMAGES: RwLock<HashMap<String, ImageState>> = RwLock::new(HashMap::new());
}

lazy_static::lazy_static! {
    pub static ref IMAGE_CACHE: ImageCache = ImageCache::new();
}

/// Convenience function to get an image (returns None if not loaded yet)
pub fn get_image(url: &str) -> Option<(Arc<Vec<u8>>, u32, u32)> {
    // First check the global loaded images
    {
        let loaded = LOADED_IMAGES.read();
        if let Some(ImageState::Loaded(pixels, w, h)) = loaded.get(url) {
            return Some((pixels.clone(), *w, *h));
        }
    }

    // Then check the main cache
    match IMAGE_CACHE.get(url) {
        ImageState::Loaded(pixels, w, h) => Some((pixels, w, h)),
        _ => None,
    }
}

/// Request loading an image
pub fn request_image(url: &str, runtime: &tokio::runtime::Handle) {
    IMAGE_CACHE.request_load(url.to_string(), runtime.clone());
}

/// Update the image cache (call from main loop)
pub fn update_cache() {
    IMAGE_CACHE.update();
}
