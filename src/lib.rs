// SPDX-License-Identifier: MIT OR Apache-2.0

//! # COSMIC Text
//!
//! This library provides advanced text handling in a generic way. It provides abstractions for
//! shaping, font discovery, font fallback, layout, rasterization, and editing. Shaping utilizes
//! rustybuzz, font discovery utilizes fontdb, and the rasterization is optional and utilizes
//! swash. The other features are developed internal to this library.
//!
//! It is recommended that you start by creating a [`FontSystem`], after which you can create a
//! [`Buffer`], provide it with some text, and then inspect the layout it produces. At this
//! point, you can use the `SwashCache` to rasterize glyphs into either images or pixels.
//!
//! ```
//! use cosmic_text::{Attrs, Color, FontSystem, SwashCache, Buffer, Metrics, Shaping};
//!
//! // A FontSystem provides access to detected system fonts, create one per application
//! let mut font_system = FontSystem::new();
//!
//! // A SwashCache stores rasterized glyphs, create one per application
//! let mut swash_cache = SwashCache::new();
//!
//! // Text metrics indicate the font size and line height of a buffer
//! let metrics = Metrics::new(14.0, 20.0);
//!
//! // A Buffer provides shaping and layout for a UTF-8 string, create one per text widget
//! let mut buffer = Buffer::new(&mut font_system, metrics);
//!
//! // Borrow buffer together with the font system for more convenient method calls
//! let mut buffer = buffer.borrow_with(&mut font_system);
//!
//! // Set a size for the text buffer, in pixels
//! buffer.set_size(80.0, 25.0);
//!
//! // Attributes indicate what font to choose
//! let attrs = Attrs::new();
//!
//! // Add some text!
//! buffer.set_text("Hello, Rust! 🦀\n", attrs, Shaping::Advanced);
//!
//! // Perform shaping as desired
//! buffer.shape_until_scroll(true);
//!
//! // Inspect the output runs
//! for run in buffer.layout_runs() {
//!     for glyph in run.glyphs.iter() {
//!         println!("{:#?}", glyph);
//!     }
//! }
//!
//! // Create a default text color
//! let text_color = Color::rgb(0xFF, 0xFF, 0xFF);
//!
//! // Draw the buffer (for performance, instead use SwashCache directly)
//! buffer.draw(&mut swash_cache, text_color, |x, y, w, h, color| {
//!     // Fill in your code here for drawing rectangles
//! });
//! ```

// Not interested in these lints
#![allow(clippy::new_without_default)]
// TODO: address occurrences and then deny
//
// Overflows can produce unpredictable results and are only checked in debug builds
#![allow(clippy::arithmetic_side_effects)]
// Indexing a slice can cause panics and that is something we always want to avoid
#![allow(clippy::indexing_slicing)]
// Soundness issues
//
// Dereferencing unaligned pointers may be undefined behavior
#![deny(clippy::cast_ptr_alignment)]
// Avoid panicking in without information about the panic. Use expect
#![deny(clippy::unwrap_used)]
// Ensure all types have a debug impl
#![deny(missing_debug_implementations)]
// This is usually a serious issue - a missing import of a define where it is interpreted
// as a catch-all variable in a match, for example
#![deny(unreachable_patterns)]
// Ensure that all must_use results are used
// #![deny(unused_must_use)]
// Style issues
//
// Documentation not ideal
#![warn(clippy::doc_markdown)]
// Document possible errors
#![warn(clippy::missing_errors_doc)]
// Document possible panics
#![warn(clippy::missing_panics_doc)]
// Ensure semicolons are present
#![warn(clippy::semicolon_if_nothing_returned)]
// Ensure numbers are readable
#![warn(clippy::unreadable_literal)]
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[cfg(not(any(feature = "std", feature = "no_std")))]
compile_error!("Either the `std` or `no_std` feature must be enabled");

use fontdb::ID;

pub use self::attrs::*;
mod attrs;

pub use self::bidi_para::*;
mod bidi_para;

pub use self::buffer::*;
mod buffer;

pub use self::buffer_line::*;
mod buffer_line;

pub use self::glyph_cache::*;
mod glyph_cache;

pub use self::cursor::*;
mod cursor;

pub use self::edit::*;
mod edit;

pub use self::font::*;
mod font;

pub use self::layout::*;
mod layout;

pub use self::line_ending::*;
mod line_ending;

pub use self::shape::*;
mod shape;

use self::shape_plan_cache::*;
mod shape_plan_cache;

pub use self::shape_run_cache::*;
mod shape_run_cache;

#[cfg(feature = "swash")]
pub use self::swash::*;
#[cfg(feature = "swash")]
mod swash;

mod math;

type BuildHasher = core::hash::BuildHasherDefault<rustc_hash::FxHasher>;

#[cfg(feature = "std")]
type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasher>;
#[cfg(not(feature = "std"))]
type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasher>;


#[derive(Debug)]
#[repr(C)]
pub struct ByteBuffer {
    ptr: *mut u8,
    length: i32,
    capacity: i32, 
}

impl ByteBuffer {
    pub fn len(&self) -> usize {
        self.length.try_into().expect("buffer length negative or overflowed")
    }

    pub fn from_vec(bytes: Vec<u8>) -> Self {
        let length = i32::try_from(bytes.len()).expect("buffer length cannot fit into a i32.");
        let capacity = i32::try_from(bytes.capacity()).expect("buffer capacity cannot fit into a i32.");

        // keep memory until call delete
        let mut v = std::mem::ManuallyDrop::new(bytes);

        Self {
            ptr: v.as_mut_ptr(),
            length,
            capacity,
        }
    }

    pub fn from_vec_struct<T: Sized>(bytes: Vec<T>) -> Self {
        let element_size = std::mem::size_of::<T>() as i32;

        let length = (bytes.len() as i32) * element_size;
        let capacity = (bytes.capacity() as i32) * element_size;

        let mut v = std::mem::ManuallyDrop::new(bytes);

        Self {
            ptr: v.as_mut_ptr() as *mut u8,
            length,
            capacity,
        }
    }

    pub fn destroy_into_vec(self) -> Vec<u8> {
        if self.ptr.is_null() {
            vec![]
        } else {
            let capacity: usize = self.capacity.try_into().expect("buffer capacity negative or overflowed");
            let length: usize = self.length.try_into().expect("buffer length negative or overflowed");

            unsafe { Vec::from_raw_parts(self.ptr, length, capacity) }
        }
    }

    pub fn destroy_into_vec_struct<T: Sized>(self) -> Vec<T> {
        if self.ptr.is_null() {
            vec![]
        } else {
            let element_size = std::mem::size_of::<T>() as i32;
            let length = (self.length * element_size) as usize;
            let capacity = (self.capacity * element_size) as usize;

            unsafe { Vec::from_raw_parts(self.ptr as *mut T, length, capacity) }
        }
    }

    pub fn destroy(self) {
        drop(self.destroy_into_vec());
    }
}

// FontSystem ---------------------------------------------------------
#[no_mangle]
pub extern "C" fn fontsystem_new() -> *mut FontSystem {
    let font_system = FontSystem::new();
    let ctx: Box<_> = Box::new(font_system);
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn fontsystem_load_system_fonts(ctx: *mut FontSystem) {
    let font_system: &mut FontSystem = unsafe { &mut *ctx };
    font_system.db_mut().load_system_fonts();
}

#[no_mangle]
pub extern "C" fn fontsystem_register_font(ctx: *mut FontSystem, font_data: *const u8, font_data_len: usize) {
    let font_system = unsafe { &mut *ctx };
    
    let font_data = unsafe { std::slice::from_raw_parts(font_data, font_data_len) };
    let font_data = Vec::from(font_data);

    font_system.db_mut().load_font_data(font_data);
}

#[no_mangle]
pub extern "C" fn fontsystem_get_font(ctx: *mut FontSystem, font_id: ID) -> *const Font {
    let font_system = unsafe { &mut *ctx };
    let font = font_system.get_font(font_id);

    if font.is_none() {
        return std::ptr::null_mut();
    }

    let font: alloc::sync::Arc<Font> = font.unwrap();
    let font: *const Font = alloc::sync::Arc::into_raw(font);
    return font;
}

#[no_mangle]
pub extern "C" fn fontsystem_free(ctx: *mut FontSystem) {
    unsafe { Box::from_raw(ctx) };
}
// ---------------------------------------------------------


// SwashCache ---------------------------------------------------------
#[no_mangle]
pub extern "C" fn swashcache_new() -> *mut SwashCache {
    let swash_cache = SwashCache::new();
    let ctx: Box<_> = Box::new(swash_cache);
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn swashcache_free(ctx: *mut SwashCache) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        Box::from_raw(ctx);
    }
}

#[no_mangle]
pub extern "C" fn swashcache_get_image_uncached(ctx: *mut SwashCache, font_system: *mut FontSystem, cache_key: CacheKey, outSwashImage: *mut SwashImage) -> bool {
    let swash_cache = unsafe { &mut *ctx };
    let font_system = unsafe { &mut *font_system };
    let imageMaybe = swash_cache.get_image_uncached(font_system, cache_key);

    if imageMaybe.is_none() {
        return false;
    }

    let image = imageMaybe.unwrap();

    let dataByteBuffer = ByteBuffer::from_vec(image.data.clone());

    let swashImage = SwashImage {
        data: dataByteBuffer,
        content: image.content,
        placement: image.placement,
        //source: image.source,
    };

    unsafe { *outSwashImage = swashImage; }
    return true;
}

#[derive(Debug)]
#[repr(C)]
pub struct SwashImage {
    pub data: ByteBuffer,
    pub content: SwashContent,
    pub placement: Placement,
    //pub source: ::swash::scale::Source,
}

// SwashImage ---------------------------------------------------------

#[no_mangle]
pub extern "C" fn swashimage_free(image: SwashImage) {
    image.data.destroy();
}

// ---------------------------------------------------------

// ---------------------------------------------------------

// Metrics ---------------------------------------------------------
#[no_mangle]
pub extern "C" fn metrics_new(font_size: f32, line_height: f32) -> *mut Metrics {
    let metrics = Metrics::new(font_size, line_height);
    let ctx: Box<_> = Box::new(metrics);
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn metrics_free(ctx: *mut Metrics) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        Box::from_raw(ctx);
    }
}
// ---------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Family<'a> {
    /// The name of a font family of choice.
    ///
    /// This must be a *Typographic Family* (ID 16) or a *Family Name* (ID 1) in terms of TrueType.
    /// Meaning you have to pass a family without any additional suffixes like _Bold_, _Italic_,
    /// _Regular_, etc.
    ///
    /// Localized names are allowed.
    Name(&'a str),

    /// Serif fonts represent the formal text style for a script.
    Serif,

    /// Glyphs in sans-serif fonts, as the term is used in CSS, are generally low contrast
    /// and have stroke endings that are plain — without any flaring, cross stroke,
    /// or other ornamentation.
    SansSerif,

    /// Glyphs in cursive fonts generally use a more informal script style,
    /// and the result looks more like handwritten pen or brush writing than printed letterwork.
    Cursive,

    /// Fantasy fonts are primarily decorative or expressive fonts that
    /// contain decorative or expressive representations of characters.
    Fantasy,

    /// The sole criterion of a monospace font is that all glyphs have the same fixed width.
    Monospace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct PrimAttrs {
    pub color: Color,
    pub family: *const u16,
    pub family_len: usize,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
    pub metadata: usize,
    pub cache_key_flags: CacheKeyFlags,
}

// Buffer ---------------------------------------------------------
#[no_mangle]
pub extern "C" fn buffer_new(font_system: *mut FontSystem, metrics: *mut Metrics) -> *mut Buffer {
    let font_system = unsafe { &mut *font_system };
    let metrics = unsafe { &mut *metrics };
    let buffer = Buffer::new(font_system, *metrics);
    let ctx: Box<_> = Box::new(buffer);
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn buffer_free(ctx: *mut Buffer) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        Box::from_raw(ctx);
    }
}

#[no_mangle]
pub extern "C" fn buffer_set_size(ctx: *mut Buffer, font_system: *mut FontSystem, width: f32, height: f32) {
    let buffer = unsafe { &mut *ctx };
    let font_system = unsafe { &mut *font_system };
    buffer.set_size(font_system, width, height);
}

#[no_mangle]
pub extern "C" fn buffer_set_text(ctx: *mut Buffer, font_system: *mut FontSystem, text: *const u16, len: usize, prim_attrs: PrimAttrs, shaping: Shaping) {
    let buffer = unsafe { &mut *ctx };

    let family_str;

    let font_family: fontdb::Family = match prim_attrs.family_len {
        0 => fontdb::Family::Serif,
        _ => {
            let family_text = unsafe { std::slice::from_raw_parts(prim_attrs.family, prim_attrs.family_len) };
            family_str = String::from_utf16(family_text).unwrap();
            fontdb::Family::Name(&family_str)
        }
    };

    let attrs = Attrs {
        color_opt: Some(prim_attrs.color),
        family: font_family,
        stretch: prim_attrs.stretch,
        style: prim_attrs.style,
        weight: prim_attrs.weight,
        metadata: prim_attrs.metadata,
        cache_key_flags: prim_attrs.cache_key_flags,
    };

    let slice = unsafe { std::slice::from_raw_parts(text, len as usize) };
    let str = String::from_utf16(slice).unwrap();

    let font_system = unsafe { &mut *font_system };
    buffer.set_text(font_system, &str, attrs.clone(), shaping);
}

#[no_mangle]
pub extern "C" fn buffer_shape_until_scroll(ctx: *mut Buffer, font_system: *mut FontSystem, scroll: bool) {
    let font_system = unsafe { &mut *font_system };
    let buffer = unsafe { &mut *ctx };
    buffer.shape_until_scroll(font_system, scroll);
}


#[no_mangle]
pub extern "C" fn buffer_layout_runs(ctx: *mut Buffer, callback: extern "C" fn(*const LayoutRun)) {
    let buffer = unsafe { &mut *ctx };
    for run in buffer.layout_runs() {
        callback(&run);
    }
}

#[no_mangle]
pub extern "C" fn buffer_draw(ctx: *mut Buffer, font_system: *mut FontSystem, swash_cache: *mut SwashCache, color: Color, callback: extern "C" fn(i32, i32, u32, u32, Color)) {
    let buffer = unsafe { &mut *ctx };
    let swash_cache = unsafe { &mut *swash_cache };
    let font_system = unsafe { &mut *font_system };
    buffer.draw(font_system, swash_cache, color, |x, y, w, h, color| {
        callback(x, y, w, h, color);
    });
}



// ---------------------------------------------------------


// LayoutRun ---------------------------------------------------------

#[no_mangle]
pub extern "C" fn layout_get_line_i(ctx: *const LayoutRun) -> usize {
    let run = unsafe { &*ctx };
    run.line_i
}

#[no_mangle]
pub extern "C" fn layout_get_text(ctx: *const LayoutRun) -> *const u8 {
    let run = unsafe { &*ctx };
    run.text.as_ptr()
}

#[no_mangle]
pub extern "C" fn layout_get_text_len(ctx: *const LayoutRun) -> usize {
    let run = unsafe { &*ctx };
    run.text.len()
}

#[no_mangle]
pub extern "C" fn layout_get_rtl(ctx: *const LayoutRun) -> bool {
    let run = unsafe { &*ctx };
    run.rtl
}

#[no_mangle]
pub extern "C" fn layout_get_glyphs(ctx: *const LayoutRun) -> *const LayoutGlyph {
    let run = unsafe { &*ctx };
    run.glyphs.as_ptr()
}

#[no_mangle]
pub extern "C" fn layout_get_glyphs_len(ctx: *const LayoutRun) -> usize {
    let run = unsafe { &*ctx };
    run.glyphs.len()
}

#[no_mangle]
pub extern "C" fn layout_get_line_y(ctx: *const LayoutRun) -> f32 {
    let run = unsafe { &*ctx };
    run.line_y
}

#[no_mangle]
pub extern "C" fn layout_get_line_top(ctx: *const LayoutRun) -> f32 {
    let run = unsafe { &*ctx };
    run.line_top
}

#[no_mangle]
pub extern "C" fn layout_get_line_w(ctx: *const LayoutRun) -> f32 {
    let run = unsafe { &*ctx };
    run.line_w
}

// ---------------------------------------------------------
