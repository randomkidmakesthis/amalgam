//! What the title's manifest (`MicrosoftGame.config` in its folder) says
//! about how it is shown, for the window it is presented in: the game's
//! display name for the caption, and its logos (`ShellVisuals`) for the
//! window's small icon (the caption's) and its big one (the taskbar's,
//! Alt-Tab's).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{E_INVALIDARG, GENERIC_READ, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL, ICONINFO,
    PostMessageW, SM_CXICON, SM_CXSMICON, WM_SETICON,
};
use windows::core::{HSTRING, Interface};

/// The title's folder, as the runtime handed it over ([`use_title_dir`]).
static TITLE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The title's folder: where its executable and `MicrosoftGame.config`
/// are, and the logos the manifest names. The first call counts.
pub fn use_title_dir(dir: PathBuf) {
    let _ = TITLE_DIR.set(dir);
}

/// The manifest's text, read once (`None`: no folder handed over, or no
/// manifest there).
fn manifest() -> Option<&'static str> {
    static TEXT: OnceLock<Option<String>> = OnceLock::new();
    TEXT.get_or_init(|| {
        let bytes = std::fs::read(TITLE_DIR.get()?.join("MicrosoftGame.config")).ok()?;
        Some(decode(&bytes))
    })
    .as_deref()
}

/// A manifest's bytes as text: UTF-16 (little-endian) after its byte
/// order mark, else UTF-8 (a mark dropped).
fn decode(bytes: &[u8]) -> String {
    if let Some(wide) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let units: Vec<u16> = wide
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let text = String::from_utf8_lossy(bytes);
    text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
}

/// Attribute `name` of the manifest's `ShellVisuals` element, its
/// predefined entities decoded; `None` if absent or blank. Comments are
/// skipped: a manifest may hold a commented-out element.
fn shell_visual(text: &str, name: &str) -> Option<String> {
    let mut rest = text;
    let tag = loop {
        let at = rest.find('<')?;
        rest = &rest[at..];
        if rest.starts_with("<!--") {
            rest = &rest[rest.find("-->")? + 3..];
            continue;
        }
        let end = rest.find('>')?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag
            .strip_prefix("ShellVisuals")
            .is_some_and(|t| t.starts_with(char::is_whitespace))
        {
            break tag;
        }
    };
    let mut attributes = tag;
    loop {
        let at = attributes.find(name)?;
        let before = attributes[..at].chars().next_back();
        let after = attributes[at + name.len()..].trim_start();
        attributes = &attributes[at + name.len()..];
        // Only the whole name (not the end of `OverrideDisplayName` for
        // `DisplayName`), followed by `=` and a quoted value.
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=').map(str::trim_start) else {
            continue;
        };
        let quote = value.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let value = &value[1..value[1..].find(quote)? + 1];
        let value = value
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        return Some(value).filter(|v| !v.trim().is_empty());
    }
}

/// The name a manifest gives the game: `OverrideDisplayName`, else
/// `DefaultDisplayName` of `ShellVisuals` (not a reference into the
/// package's resources, `ms-resource:`, which only the store resolves).
fn display_name_in(text: &str) -> Option<String> {
    ["OverrideDisplayName", "DefaultDisplayName"]
        .iter()
        .filter_map(|name| shell_visual(text, name))
        .find(|name| !name.starts_with("ms-resource:"))
        .map(|name| name.trim().to_owned())
}

/// The game's display name from its manifest, read once.
pub fn display_name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| display_name_in(manifest()?)).as_deref()
}

/// The window's caption: product, game, frame rate and time per frame
/// over the last interval (`fps` 0: no frame was presented in it), draws
/// per frame. The game's field is left out when there is no name.
pub fn caption(name: &str, fps: f64, draws: f64) -> String {
    let ms = if fps > 0.0 {
        format!("{:.1}", 1000.0 / fps)
    } else {
        "–".to_owned()
    };
    let name = name.trim();
    let game = if name.is_empty() {
        String::new()
    } else {
        format!(" | {name}")
    };
    format!("Scarlett{game} | {fps:.1} FPS | {ms} ms | {draws:.0} draws")
}

/// The game's name in a window's text: the text itself, but for a
/// caption of ours (our own window's first text, or the caption a
/// presenter made before, read again by a new one), whose game field it
/// is (none: empty).
pub fn game_in_caption(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("Scarlett") else {
        return text;
    };
    rest.strip_prefix(" | ")
        .and_then(|rest| rest.split(" | ").next())
        .filter(|field| !field.ends_with(" FPS"))
        .unwrap_or("")
}

/// Whether frames are shown (`VICTORY_PRESENT_SKIP`, default 0: every frame
/// presented as before). 1: the frame is copied to the swap chain's back
/// buffer as always but `Present` is never called (with the displays off
/// Windows throttles a present to ~150 ms; for unattended measurement).
/// N > 1: `Present` every Nth frame with `DXGI_PRESENT_DO_NOT_WAIT`, its
/// result ignored, the other frames not presented.
pub fn present_skip() -> u64 {
    static N: OnceLock<u64> = OnceLock::new();
    *N.get_or_init(|| {
        let n = std::env::var("VICTORY_PRESENT_SKIP").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
        match n {
            0 => {}
            1 => tracing::info!("presentation: frames are not shown (VICTORY_PRESENT_SKIP=1): for unattended measurement with the display off"),
            n => tracing::info!(
                "presentation: one frame in {n} is shown, without waiting (VICTORY_PRESENT_SKIP={n}, DXGI_PRESENT_DO_NOT_WAIT): for unattended measurement with the display off"
            ),
        }
        n
    })
}

/// The host's present of a frame under [`present_skip`]: `present(flags)`
/// is called as before (0), never (1), or every Nth frame with
/// `DXGI_PRESENT_DO_NOT_WAIT` (`do_not_wait` is that flag's value), its
/// result ignored. Returns what the call returned, if it was made.
pub fn present_maybe<R>(do_not_wait: u32, present: impl FnOnce(u32) -> R) -> Option<R> {
    static PRESENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    match present_skip() {
        0 => Some(present(0)),
        1 => None,
        n => (PRESENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % n == 0).then(|| present(do_not_wait)),
    }
}

/// The caption text for the window, from what the window said when first
/// read (`first`, the title's caption): `Scarlett | <game> | F FPS | M ms |
/// D draws`, the game's name from the manifest, else from `first`. The
/// form our scripts parse (fields split at `|`).
pub fn caption_for(first: &[u16], fps: f64, draws: f64) -> String {
    let name = match display_name() {
        Some(name) => name.to_owned(),
        None => game_in_caption(&String::from_utf16_lossy(first)).to_owned(),
    };
    caption(&name, fps, draws)
}

/// The logo files in `dir` for the small and the big icon, of those the
/// manifest names (`attribute` reads them) that exist: the 44-pixel logo
/// for the small one and the 150-pixel one for the big one, each falling
/// back to the other, then to the 480-pixel one.
fn logos(dir: &Path, attribute: impl Fn(&str) -> Option<String>) -> [Option<PathBuf>; 2] {
    let logo = |name: &str| {
        attribute(name)
            .map(|file| dir.join(file.trim()))
            .filter(|path| path.is_file())
    };
    let [s44, s150, s480] = ["Square44x44Logo", "Square150x150Logo", "Square480x480Logo"].map(logo);
    [
        s44.clone()
            .or_else(|| s150.clone())
            .or_else(|| s480.clone()),
        s150.or(s480).or(s44),
    ]
}

/// Whether the window gets the game's logos as its icons; on by default,
/// `VICTORY_WINDOW_ICON=0` leaves the window's own.
fn icons_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_WINDOW_ICON").is_none_or(|v| v != "0"))
}

/// The icons given to each window (handles as numbers), destroyed with
/// it ([`window_destroyed`]): a window does not own the icons it is given.
static ICONS: Mutex<Vec<(usize, [usize; 2])>> = Mutex::new(Vec::new());

/// Gives `hwnd` the game's logos as its small and big icons, on a thread
/// of its own: decoding them takes a few milliseconds and is nothing the
/// window or presenting waits for. Whatever fails, the window keeps the
/// icon it has and one line says why.
pub fn set_icons(hwnd: HWND) {
    if !icons_on() {
        return;
    }
    let hwnd = hwnd.0 as usize;
    let _ = std::thread::Builder::new()
        .name("victory-icons".into())
        .spawn(move || {
            if let Err(why) = make_icons(hwnd) {
                tracing::info!("the window keeps its icon: {why}");
            }
        });
}

/// [`set_icons`]' work, on its thread.
fn make_icons(hwnd: usize) -> Result<(), String> {
    let dir = TITLE_DIR.get().ok_or("the title's folder is not known")?;
    let text = manifest().ok_or("the title has no MicrosoftGame.config")?;
    let paths = logos(dir, |name| shell_visual(text, name));
    if paths.iter().all(Option::is_none) {
        return Err("the manifest names no logo that exists (Square44x44Logo, \
                    Square150x150Logo, Square480x480Logo)"
            .into());
    }
    // SAFETY: plain queries.
    let sizes = unsafe { [GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CXICON)] };
    // This thread is ours and does nothing else with COM: initialised here
    // for the decoding, and uninitialised only if this initialised it
    // (`S_FALSE`, already initialised the same way, counts too).
    // SAFETY: COM for this thread, undone below.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut icons = [0usize; 2];
    let mut failed = Vec::new();
    for (k, path) in paths.iter().enumerate() {
        let Some(path) = path else { continue };
        // SAFETY: COM is initialised on this thread (or was already).
        match unsafe { icon_from_image(path, sizes[k]) } {
            Ok(icon) => icons[k] = icon.0 as usize,
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        }
    }
    if com {
        // SAFETY: undoes the initialisation above.
        unsafe { CoUninitialize() };
    }
    if icons == [0, 0] {
        return Err(format!(
            "no logo could be made an icon ({})",
            failed.join("; ")
        ));
    }
    // Noted before they are handed over, so that a window destroyed
    // meanwhile frees them.
    ICONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((hwnd, icons));
    // Posted, not sent: the window's thread is the title's, which may not
    // pump for a while. (A handle, no pointer, travels in the message.)
    for (k, which) in [ICON_SMALL, ICON_BIG].into_iter().enumerate() {
        if icons[k] == 0 {
            continue;
        }
        // SAFETY: a window handle (a stale one fails) and our icon.
        let posted = unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_SETICON,
                WPARAM(which as usize),
                LPARAM(icons[k] as isize),
            )
        };
        if posted.is_err() {
            window_destroyed(hwnd);
            return Err("the window is gone".into());
        }
    }
    tracing::info!(
        small = sizes[0],
        big = sizes[1],
        "the window's icons are the game's logos{}",
        if failed.is_empty() {
            String::new()
        } else {
            format!(" (not: {})", failed.join("; "))
        }
    );
    Ok(())
}

/// Window `hwnd` is being destroyed (`WM_NCDESTROY`): the icons
/// [`set_icons`] gave it are freed.
pub fn window_destroyed(hwnd: usize) {
    let gone: Vec<[usize; 2]> = {
        let mut icons = ICONS.lock().unwrap_or_else(|e| e.into_inner());
        let (gone, kept) = std::mem::take(&mut *icons)
            .into_iter()
            .partition(|i| i.0 == hwnd);
        *icons = kept;
        gone.into_iter().map(|(_, pair)| pair).collect()
    };
    for icon in gone.into_iter().flatten().filter(|&i| i != 0) {
        // SAFETY: an icon of ours, given to that window only.
        let _ = unsafe { DestroyIcon(HICON(icon as *mut _)) };
    }
}

/// The image at `path` (a PNG, or anything Windows Imaging Component
/// decodes) as an icon `size` pixels square (0: the image's own size).
///
/// # Safety
/// COM must be initialised on the calling thread.
unsafe fn icon_from_image(path: &Path, size: i32) -> windows::core::Result<HICON> {
    // SAFETY: COM calls on objects made here; the buffer is as large as
    // the stride and height say.
    unsafe {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = wic.CreateDecoderFromFilename(
            &HSTRING::from(path.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let mut source: IWICBitmapSource = decoder.GetFrame(0)?.cast()?;
        let (mut width, mut height) = (0u32, 0u32);
        source.GetSize(&mut width, &mut height)?;
        if size > 0 && (width, height) != (size as u32, size as u32) {
            let scaler = wic.CreateBitmapScaler()?;
            scaler.Initialize(
                &source,
                size as u32,
                size as u32,
                WICBitmapInterpolationModeFant,
            )?;
            source = scaler.cast()?;
        }
        let bgra = wic.CreateFormatConverter()?;
        bgra.Initialize(
            &source,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None::<&IWICPalette>,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        bgra.GetSize(&mut width, &mut height)?;
        // An icon of more than 1024 pixels a side is no logo.
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Err(E_INVALIDARG.into());
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        bgra.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
        icon_from_bgra(width as i32, height as i32, &pixels)
    }
}

/// An icon from `width` x `height` 32-bit BGRA pixels, top row first,
/// alpha not premultiplied: a DIB section of them, and a mask that is all
/// zeros (the alpha says what is transparent).
fn icon_from_bgra(width: i32, height: i32, pixels: &[u8]) -> windows::core::Result<HICON> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top row first.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let bytes = width as usize * height as usize * 4;
    if pixels.len() < bytes {
        return Err(E_INVALIDARG.into());
    }
    // SAFETY: GDI objects made and freed here; the section holds `bytes`
    // bytes, the mask's bits a row of whole words per line.
    unsafe {
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        if bits.is_null() {
            let _ = DeleteObject(color.into());
            return Err(E_INVALIDARG.into());
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), bytes);
        let mask_bits = vec![0u8; (width as usize).div_ceil(16) * 2 * height as usize];
        let mask = CreateBitmap(width, height, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        // The icon holds copies of both.
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caption_says_frame_rate_and_time_per_frame() {
        assert_eq!(
            caption("Grand Theft Auto VI", 8.9, 4225.4),
            "Scarlett | Grand Theft Auto VI | 8.9 FPS | 112.4 ms | 4225 draws"
        );
        assert_eq!(
            caption("Americas", 25.0, 10.0),
            "Scarlett | Americas | 25.0 FPS | 40.0 ms | 10 draws"
        );
        // No frame presented in the interval.
        assert_eq!(
            caption("Americas", 0.0, 0.0),
            "Scarlett | Americas | 0.0 FPS | – ms | 0 draws"
        );
        // No name: the field is left out.
        assert_eq!(
            caption("", 30.0, 1.0),
            "Scarlett | 30.0 FPS | 33.3 ms | 1 draws"
        );
        // The name in a window's text, also in a caption of ours.
        assert_eq!(game_in_caption("Americas"), "Americas");
        assert_eq!(game_in_caption("Scarlett"), "");
        assert_eq!(game_in_caption(&caption("Americas", 9.0, 1.0)), "Americas");
        assert_eq!(game_in_caption(&caption("", 9.0, 1.0)), "");
    }

    #[test]
    fn the_manifest_names_the_game_and_its_logos() {
        let text = "\u{feff}<?xml version=\"1.0\"?>\n<Game configVersion=\"1\">\n\
            <!-- <ShellVisuals DefaultDisplayName=\"Old\" /> -->\n\
            <ShellVisuals DefaultDisplayName=\"Grand Theft Auto VI\" \
            PublisherDisplayName=\"Rockstar Games\" Square150x150Logo=\"xbs_logo_150.png\" \
            Square44x44Logo='xbs_logo_44.png' Description=\"A &amp; B\" StoreLogo=\"\" />\n</Game>";
        assert_eq!(
            display_name_in(text).as_deref(),
            Some("Grand Theft Auto VI")
        );
        assert_eq!(
            shell_visual(text, "Square44x44Logo").as_deref(),
            Some("xbs_logo_44.png")
        );
        assert_eq!(shell_visual(text, "DisplayName"), None, "only whole names");
        assert_eq!(shell_visual(text, "Description").as_deref(), Some("A & B"));
        assert_eq!(shell_visual(text, "StoreLogo"), None, "blank");
        assert_eq!(shell_visual(text, "Square480x480Logo"), None, "absent");
        let renamed = text.replace(
            "<ShellVisuals ",
            "<ShellVisuals OverrideDisplayName=\"GTA VI\" ",
        );
        assert_eq!(display_name_in(&renamed).as_deref(), Some("GTA VI"));
        let resource = text.replace("Grand Theft Auto VI", "ms-resource:Name");
        assert_eq!(display_name_in(&resource), None);
        // UTF-16 with its mark, as some manifests are saved.
        let wide: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(
                text.trim_start_matches('\u{feff}')
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        assert_eq!(
            display_name_in(&decode(&wide)).as_deref(),
            Some("Grand Theft Auto VI")
        );
    }

    #[test]
    fn logos_fall_back_to_those_that_exist() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let names = |name: &str| {
            let file = match name {
                "Square44x44Logo" => "s44.png",
                "Square150x150Logo" => "s150.png",
                "Square480x480Logo" => "s480.png",
                _ => return None,
            };
            Some(file.to_owned())
        };
        assert_eq!(logos(dir, names), [None, None]);
        std::fs::write(dir.join("s480.png"), b"x").unwrap();
        assert_eq!(
            logos(dir, names),
            [Some(dir.join("s480.png")), Some(dir.join("s480.png"))]
        );
        std::fs::write(dir.join("s44.png"), b"x").unwrap();
        assert_eq!(
            logos(dir, names),
            [Some(dir.join("s44.png")), Some(dir.join("s480.png"))]
        );
        std::fs::write(dir.join("s150.png"), b"x").unwrap();
        assert_eq!(
            logos(dir, names),
            [Some(dir.join("s44.png")), Some(dir.join("s150.png"))]
        );
        std::fs::remove_file(dir.join("s44.png")).unwrap();
        assert_eq!(
            logos(dir, names),
            [Some(dir.join("s150.png")), Some(dir.join("s150.png"))]
        );
        assert_eq!(logos(dir, |_| None), [None, None]);
    }

    /// A PNG of `width` x `height` RGBA pixels: its image data stored in
    /// zlib without compression.
    fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        fn crc(bytes: &[u8]) -> u32 {
            let mut c = !0u32;
            for &b in bytes {
                c ^= u32::from(b);
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xedb8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let start = out.len();
            out.extend(kind);
            out.extend(data);
            let sum = crc(&out[start..]);
            out.extend(sum.to_be_bytes());
        }
        // Each row after its filter byte (0: none).
        let raw: Vec<u8> = rgba
            .chunks(width as usize * 4)
            .flat_map(|row| std::iter::once(0).chain(row.iter().copied()))
            .collect();
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in &raw {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
        let len = raw.len() as u16;
        let mut zlib = vec![0x78, 0x01, 0x01];
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend(&raw);
        zlib.extend((b << 16 | a).to_be_bytes());
        let mut header = Vec::new();
        header.extend(width.to_be_bytes());
        header.extend(height.to_be_bytes());
        // 8 bits a channel, RGBA, deflate, adaptive filtering, no interlace.
        header.extend([8, 6, 0, 0, 0]);
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut out, b"IHDR", &header);
        chunk(&mut out, b"IDAT", &zlib);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    /// A small PNG in the temporary folder becomes an icon of the asked
    /// size (or its own); bytes that are no image, and no file, do not.
    #[test]
    fn a_png_becomes_an_icon_and_garbage_does_not() {
        use windows::Win32::Graphics::Gdi::{BITMAP, GetObjectW};
        use windows::Win32::UI::WindowsAndMessaging::GetIconInfo;
        let dir = tempfile::tempdir().unwrap();
        // 2x2: red, half-transparent green, blue, white.
        let pixels = [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        std::fs::write(dir.path().join("logo.png"), png(2, 2, &pixels)).unwrap();
        std::fs::write(dir.path().join("bad.png"), b"no image at all").unwrap();
        // SAFETY: COM for this test's thread, undone at the end; the icons
        // and their bitmaps are freed here.
        unsafe {
            let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            for size in [0, 16, 32] {
                let icon = icon_from_image(&dir.path().join("logo.png"), size).expect("decoded");
                let mut info = ICONINFO::default();
                GetIconInfo(icon, &mut info).unwrap();
                let mut bitmap = BITMAP::default();
                GetObjectW(
                    info.hbmColor.into(),
                    size_of::<BITMAP>() as i32,
                    Some((&raw mut bitmap).cast()),
                );
                let want = if size == 0 { 2 } else { size };
                assert_eq!((bitmap.bmWidth, bitmap.bmHeight.abs()), (want, want));
                let _ = DeleteObject(info.hbmColor.into());
                let _ = DeleteObject(info.hbmMask.into());
                DestroyIcon(icon).unwrap();
            }
            assert!(icon_from_image(&dir.path().join("bad.png"), 16).is_err());
            assert!(icon_from_image(&dir.path().join("missing.png"), 16).is_err());
            if com {
                CoUninitialize();
            }
        }
    }
}
