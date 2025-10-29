//! Discovers system-native styling for colors, fonts, and other metrics.
//!
//! This module provides a best-effort attempt to query the host operating system
//! for its UI theme information. This is gated behind the **`io`** feature flag.
//!
//! **Application-Specific Ricing:**
//! By default (if the `io` feature is enabled), Azul will look for an application-specific
//! stylesheet at `~/.config/azul/styles/<app_name>.css` (or `%APPDATA%\azul\styles\<app_name>.css`
//! on Windows). This allows end-users to override and "rice" any Azul application.
//! This behavior can be disabled by setting the `AZUL_DISABLE_RICING` environment variable.
//!
//! **Linux Customization Easter Egg:**
//! Linux users can set the `AZUL_SMOKE_AND_MIRRORS` environment variable to force Azul to
//! skip standard GNOME/KDE detection and prioritize discovery methods for "riced" desktops
//! (like parsing Hyprland configs or `pywal` caches), leaning into the car "ricing" subculture
//! where a flashy appearance is paramount.

#![cfg(feature = "parser")]

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "io")]
use core::time::Duration;

use crate::{
    css::Stylesheet,
    parser2::{new_from_str, CssParseWarnMsg},
    props::{
        basic::{
            color::{parse_css_color, ColorU},
            pixel::PixelValue,
        },
        style::scrollbar::ComputedScrollbarStyle,
    },
};

// --- Public Data Structures ---

/// Represents the detected platform.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux(DesktopEnvironment),
    Android,
    Ios,
    #[default]
    Unknown,
}

/// Represents the detected Linux Desktop Environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Other(String),
}

/// The overall theme type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// A unified collection of discovered system style properties.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SystemStyle {
    pub theme: Theme,
    pub platform: Platform,
    pub colors: SystemColors,
    pub fonts: SystemFonts,
    pub metrics: SystemMetrics,
    pub scrollbar: Option<ComputedScrollbarStyle>,
    /// An optional, user-provided stylesheet loaded from a conventional
    /// location (`~/.config/azul/styles/<app_name>.css`), allowing for
    /// application-specific "ricing". This is only loaded when the "io"
    /// feature is enabled and not disabled by the `AZUL_DISABLE_RICING` env var.
    pub app_specific_stylesheet: Option<Box<Stylesheet>>,
}

/// Common system colors used for UI elements.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SystemColors {
    pub text: Option<ColorU>,
    pub background: Option<ColorU>,
    pub accent: Option<ColorU>,
    pub accent_text: Option<ColorU>,
    pub button_face: Option<ColorU>,
    pub button_text: Option<ColorU>,
    pub window_background: Option<ColorU>,
    pub selection_background: Option<ColorU>,
    pub selection_text: Option<ColorU>,
}

/// Common system font settings.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SystemFonts {
    /// The primary font used for UI elements like buttons and labels.
    pub ui_font: Option<String>,
    /// The default font size for UI elements, in points.
    pub ui_font_size: Option<f32>,
    /// The font used for code or other monospaced text.
    pub monospace_font: Option<String>,
}

/// Common system metrics for UI element sizing and spacing.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SystemMetrics {
    /// The corner radius for standard elements like buttons.
    pub corner_radius: Option<PixelValue>,
    /// The width of standard borders.
    pub border_width: Option<PixelValue>,
}

impl SystemStyle {
    /// Discovers the system's UI style, and loads an optional app-specific stylesheet.
    ///
    /// If the "io" feature is enabled, this function may be slow as it can
    /// involve running external commands and reading files.
    ///
    /// If the "io" feature is disabled, this returns a hard-coded, deterministic
    /// style based on the target operating system.
    pub fn new() -> Self {
        // Step 1: Get the base style (either from I/O or hardcoded defaults).
        let mut style = {
            #[cfg(feature = "io")]
            {
                #[cfg(target_os = "linux")]
                {
                    discover_linux_style()
                }
                #[cfg(target_os = "windows")]
                {
                    discover_windows_style()
                }
                #[cfg(target_os = "macos")]
                {
                    discover_macos_style()
                }
                #[cfg(target_os = "android")]
                {
                    defaults::android_material_light()
                }
                #[cfg(target_os = "ios")]
                {
                    defaults::ios_light()
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "android",
                    target_os = "ios"
                )))]
                {
                    Self::default()
                } // Fallback for unknown OS
            }
            #[cfg(not(feature = "io"))]
            {
                // Return hard-coded defaults based on compile-time target
                #[cfg(target_os = "windows")]
                {
                    defaults::windows_11_light()
                }
                #[cfg(target_os = "macos")]
                {
                    defaults::macos_modern_light()
                }
                #[cfg(target_os = "linux")]
                {
                    defaults::gnome_adwaita_light()
                }
                #[cfg(target_os = "android")]
                {
                    defaults::android_material_light()
                }
                #[cfg(target_os = "ios")]
                {
                    defaults::ios_light()
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "android",
                    target_os = "ios"
                )))]
                {
                    Self::default()
                }
            }
        };

        // Step 2: Check for the opt-out env var for app-specific styling.
        #[cfg(feature = "io")]
        {
            if std::env::var("AZUL_DISABLE_RICING").is_ok() {
                return style; // User explicitly disabled it.
            }

            // Step 3: Try to load the app-specific stylesheet.
            if let Some(stylesheet) = load_app_specific_stylesheet() {
                style.app_specific_stylesheet = Some(Box::new(stylesheet));
            }
        }

        style
    }

    /// Create a CSS stylesheet for CSD (Client-Side Decorations) titlebar
    ///
    /// This generates CSS rules for the CSD titlebar using system colors,
    /// fonts, and metrics to match the native platform look.
    pub fn create_csd_stylesheet(&self) -> Stylesheet {
        use alloc::format;

        use crate::parser2::new_from_str;

        // Build CSS string from SystemStyle
        let mut css = String::new();

        // Get system colors with fallbacks
        let bg_color = self
            .colors
            .window_background
            .unwrap_or(ColorU::new_rgb(240, 240, 240));
        let text_color = self.colors.text.unwrap_or(ColorU::new_rgb(0, 0, 0));
        let accent_color = self.colors.accent.unwrap_or(ColorU::new_rgb(0, 120, 215));
        let border_color = match self.theme {
            Theme::Dark => ColorU::new_rgb(60, 60, 60),
            Theme::Light => ColorU::new_rgb(200, 200, 200),
        };

        // Get system metrics with fallbacks
        let corner_radius = self
            .metrics
            .corner_radius
            .map(|px| format!("{}px", px.to_pixels(1.0)))
            .unwrap_or_else(|| "4px".to_string());

        // Titlebar container
        css.push_str(&format!(
            ".csd-titlebar {{ width: 100%; height: 32px; background: rgb({}, {}, {}); \
             border-bottom: 1px solid rgb({}, {}, {}); display: flex; flex-direction: row; \
             align-items: center; justify-content: space-between; padding: 0 8px; }} ",
            bg_color.r, bg_color.g, bg_color.b, border_color.r, border_color.g, border_color.b,
        ));

        // Title text
        css.push_str(&format!(
            ".csd-title {{ color: rgb({}, {}, {}); font-size: 13px; flex-grow: 1; text-align: \
             center; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }} ",
            text_color.r, text_color.g, text_color.b,
        ));

        // Button container
        css.push_str(".csd-buttons { display: flex; flex-direction: row; gap: 4px; } ");

        // Buttons
        css.push_str(&format!(
            ".csd-button {{ width: 32px; height: 24px; border-radius: {}; background: \
             transparent; color: rgb({}, {}, {}); font-size: 16px; line-height: 24px; text-align: \
             center; cursor: pointer; user-select: none; }} ",
            corner_radius, text_color.r, text_color.g, text_color.b,
        ));

        // Button hover state
        let hover_color = match self.theme {
            Theme::Dark => ColorU::new_rgb(60, 60, 60),
            Theme::Light => ColorU::new_rgb(220, 220, 220),
        };
        css.push_str(&format!(
            ".csd-button:hover {{ background: rgb({}, {}, {}); }} ",
            hover_color.r, hover_color.g, hover_color.b,
        ));

        // Close button hover (red on all platforms)
        css.push_str(
            ".csd-close:hover { background: rgb(232, 17, 35); color: rgb(255, 255, 255); } ",
        );

        // Platform-specific button styling
        match self.platform {
            Platform::MacOs => {
                // macOS traffic light buttons (left side)
                css.push_str(".csd-buttons { position: absolute; left: 8px; } ");
                css.push_str(
                    ".csd-close { background: rgb(255, 95, 86); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
                css.push_str(
                    ".csd-minimize { background: rgb(255, 189, 46); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
                css.push_str(
                    ".csd-maximize { background: rgb(40, 201, 64); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
            }
            Platform::Linux(_) => {
                // Linux - title on left, buttons on right
                css.push_str(".csd-title { text-align: left; } ");
            }
            _ => {
                // Windows and others - standard layout
            }
        }

        // Parse CSS string into Stylesheet
        let (mut parsed_css, _warnings) = new_from_str(&css);

        // Return first stylesheet (should always exist)
        if !parsed_css.stylesheets.is_empty() {
            parsed_css.stylesheets.into_library_owned_vec().remove(0)
        } else {
            Stylesheet::default()
        }
    }
}

// -- Platform-Specific Implementations (with I/O) --

#[cfg(feature = "io")]
fn discover_linux_style() -> SystemStyle {
    // Check for the easter egg env var. If it's set, we skip straight to the "riced"
    // discovery, embracing the smoke and mirrors of a custom desktop.
    if std::env::var("AZUL_SMOKE_AND_MIRRORS").is_err() {
        // If the env var is NOT set, try the normal desktop environments first.
        if let Ok(kde_style) = discover_kde_style() {
            return kde_style;
        }
        if let Ok(gnome_style) = discover_gnome_style() {
            return gnome_style;
        }
    }

    // This also acts as a fallback for non-GNOME/KDE environments.
    if let Ok(riced_style) = discover_riced_style() {
        return riced_style;
    }

    // Absolute fallback if nothing can be determined.
    defaults::gnome_adwaita_light()
}

#[cfg(feature = "io")]
fn discover_gnome_style() -> Result<SystemStyle, ()> {
    let theme_name = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
        Duration::from_secs(1),
    )?;
    let theme_name = theme_name.trim().trim_matches('\'');

    let color_scheme = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
        Duration::from_secs(1),
    )
    .unwrap_or_default();
    let theme = if color_scheme.contains("prefer-dark") {
        Theme::Dark
    } else {
        Theme::Light
    };

    let ui_font = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "font-name"],
        Duration::from_secs(1),
    )
    .ok();
    let monospace_font = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "monospace-font-name"],
        Duration::from_secs(1),
    )
    .ok();

    let mut style = if theme == Theme::Dark {
        defaults::gnome_adwaita_dark()
    } else {
        defaults::gnome_adwaita_light()
    };

    style.platform = Platform::Linux(DesktopEnvironment::Gnome);
    if let Some(font) = ui_font {
        style.fonts.ui_font = Some(font.trim().trim_matches('\'').to_string());
    }
    if let Some(font) = monospace_font {
        style.fonts.monospace_font = Some(font.trim().trim_matches('\'').to_string());
    }

    Ok(style)
}

#[cfg(feature = "io")]
fn discover_kde_style() -> Result<SystemStyle, ()> {
    // Check for kreadconfig5. If it doesn't exist, we're likely not on KDE Plasma 5+.
    run_command_with_timeout("kreadconfig5", &["--version"], Duration::from_secs(1))?;

    // Get the color scheme name to determine light/dark theme.
    let scheme_name = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "ColorScheme"],
        Duration::from_secs(1),
    )
    .unwrap_or_default();
    let theme = if scheme_name.to_lowercase().contains("dark") {
        Theme::Dark
    } else {
        Theme::Light
    };

    // Start with the appropriate Breeze default.
    let mut style = if theme == Theme::Dark {
        // NOTE: A specific "breeze_dark" default could be added for more accuracy.
        defaults::gnome_adwaita_dark()
    } else {
        defaults::kde_breeze_light()
    };
    style.platform = Platform::Linux(DesktopEnvironment::Kde);

    // Get the UI font. The format is "Font Name,Size,-1,5,50,0,0,0,0,0"
    if let Ok(font_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "font"],
        Duration::from_secs(1),
    ) {
        let mut parts = font_str.trim().split(',');
        if let Some(font_name) = parts.next() {
            style.fonts.ui_font = Some(font_name.to_string());
        }
        if let Some(font_size_str) = parts.next() {
            if let Ok(size) = font_size_str.parse::<f32>() {
                style.fonts.ui_font_size = Some(size);
            }
        }
    }

    // Get the monospace font.
    if let Ok(font_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "fixed"],
        Duration::from_secs(1),
    ) {
        if let Some(font_name) = font_str.trim().split(',').next() {
            style.fonts.monospace_font = Some(font_name.to_string());
        }
    }

    // Get the accent color (active titlebar color). Format is "R,G,B".
    if let Ok(color_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "WM", "--key", "activeBackground"],
        Duration::from_secs(1),
    ) {
        let rgb: Vec<Result<u8, _>> = color_str
            .trim()
            .split(',')
            .map(|c| c.parse::<u8>())
            .collect();
        if rgb.len() == 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (&rgb[0], &rgb[1], &rgb[2]) {
                style.colors.accent = Some(ColorU::new_rgb(*r, *g, *b));
            }
        }
    }

    Ok(style)
}

#[cfg(feature = "io")]
/// Attempts to discover styling from common "ricing" tools and window manager configs.
fn discover_riced_style() -> Result<SystemStyle, ()> {
    // We can confirm we're in a specific WM environment if needed.
    // For example, Hyprland sets this variable.
    let is_hyprland = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok();
    if !is_hyprland {
        // This function could be expanded to check for sway, i3, etc.
        // For now, we'll only proceed if we have a strong hint.
        return Err(());
    }

    let mut style = SystemStyle {
        platform: Platform::Linux(DesktopEnvironment::Other("Tiling WM".into())),
        // Start with a generic dark theme, as it's common for riced setups.
        ..defaults::gnome_adwaita_dark()
    };

    // Strategy 3: Check for a `pywal` cache first, as it's a great source for colors.
    let home_dir = std::env::var("HOME").unwrap_or_default();
    let wal_cache_path = format!("{}/.cache/wal/colors.json", home_dir);
    if let Ok(json_content) = std::fs::read_to_string(wal_cache_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_content) {
            let colors = &json["colors"];
            style.colors.background = colors["color0"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok());
            style.colors.text = colors["color7"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok());
            style.colors.accent = colors["color4"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok());
            style.theme = Theme::Dark; // Wal is often used with dark themes.
        }
    }

    // Strategy 2: Parse hyprland.conf for specifics like borders and radius.
    let hypr_conf_path = format!("{}/.config/hypr/hyprland.conf", home_dir);
    if let Ok(conf_content) = std::fs::read_to_string(hypr_conf_path) {
        for line in conf_content.lines() {
            let line = line.trim();
            if line.starts_with('#') || !line.contains('=') {
                continue;
            }
            let mut parts = line.splitn(2, '=').map(|s| s.trim());
            let key = parts.next();
            let value = parts.next();

            if let (Some(k), Some(v)) = (key, value) {
                match k {
                    "rounding" => {
                        if let Ok(px) = v.parse::<f32>() {
                            style.metrics.corner_radius = Some(PixelValue::px(px));
                        }
                    }
                    "border_size" => {
                        if let Ok(px) = v.parse::<f32>() {
                            style.metrics.border_width = Some(PixelValue::px(px));
                        }
                    }
                    // Use the active border as the accent color if `wal` didn't provide one.
                    "col.active_border" if style.colors.accent.is_none() => {
                        // Hyprland format is "rgba(RRGGBBAA)" or "rgb(RRGGBB)"
                        if let Some(hex_str) = v.split_whitespace().last() {
                            if let Ok(color) = parse_css_color(&format!("#{}", hex_str)) {
                                style.colors.accent = Some(color);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Strategy 1: Finally, try to get the GTK font as a sensible default for UI text.
    if let Ok(font_str) = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "font-name"],
        Duration::from_secs(1),
    ) {
        if let Some(font_name) = font_str.trim().trim_matches('\'').split(' ').next() {
            style.fonts.ui_font = Some(font_name.to_string());
        }
    }

    Ok(style)
}

#[cfg(feature = "io")]
fn discover_windows_style() -> SystemStyle {
    let mut style = defaults::windows_11_light(); // Start with a modern default
    style.platform = Platform::Windows;

    let theme_val = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ],
        Duration::from_secs(1),
    );
    if let Ok(output) = theme_val {
        if output.contains("0x0") {
            style = defaults::windows_11_dark();
        }
    }

    let accent_val = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\DWM",
            "/v",
            "AccentColor",
        ],
        Duration::from_secs(1),
    );
    if let Ok(output) = accent_val {
        if let Some(hex_str) = output.split_whitespace().last() {
            if let Ok(hex_val) = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
                let a = (hex_val >> 24) as u8;
                let b = (hex_val >> 16) as u8;
                let g = (hex_val >> 8) as u8;
                let r = hex_val as u8;
                style.colors.accent = Some(ColorU::new(r, g, b, a));
            }
        }
    }

    style
}

#[cfg(feature = "io")]
fn discover_macos_style() -> SystemStyle {
    let mut style = defaults::macos_modern_light();
    style.platform = Platform::MacOs;

    let theme_val = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleInterfaceStyle"],
        Duration::from_secs(1),
    );
    if theme_val.is_ok() {
        style = defaults::macos_modern_dark();
    }

    style
}

#[cfg(feature = "io")]
fn discover_android_style() -> SystemStyle {
    // On-device detection is complex; return a modern default.
    defaults::android_material_light()
}

#[cfg(feature = "io")]
fn discover_ios_style() -> SystemStyle {
    // On-device detection is complex; return a modern default.
    defaults::ios_light()
}

// -- Helper Functions (IO-dependent) --

#[cfg(feature = "io")]
/// A simple helper to run a command and get its stdout, with a timeout.
fn run_command_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Result<String, ()> {
    use std::{
        process::{Command, Stdio},
        thread,
    };

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ())?;

    let (tx, rx) = std::sync::mpsc::channel();

    let child_thread = thread::spawn(move || {
        let output = child.wait_with_output();
        tx.send(output).ok();
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Ok(String::from_utf8(output.stdout).unwrap_or_default())
        }
        _ => {
            // Ensure the child process is killed on timeout
            // This part is tricky without a more robust process management library
            child_thread.join().ok(); // Wait for the thread to finish
            Err(())
        }
    }
}

/// Loads an application-specific stylesheet from a conventional path.
///
/// Looks for `<config_dir>/azul/styles/<exe_name>.css`.
/// Returns `None` if the file doesn't exist, can't be read, or is empty.
#[cfg(feature = "io")]
/// Loads an application-specific stylesheet from a conventional path.
///
/// Looks for `<config_dir>/azul/styles/<exe_name>.css`.
/// Returns `None` if the file doesn't exist, can't be read, or is empty.
#[cfg(feature = "io")]
fn load_app_specific_stylesheet() -> Option<Stylesheet> {
    // Get the name of the currently running executable.
    let exe_path = std::env::current_exe().ok()?;
    let exe_name = exe_path.file_name()?.to_str()?;

    // Use `dirs-next` to find the conventional config directory for the current platform.
    // This correctly handles Linux ($XDG_CONFIG_HOME, ~/.config),
    // macOS (~/Library/Application Support), and Windows (%APPDATA%).
    let config_dir = dirs_next::config_dir()?;

    let css_path = config_dir
        .join("azul")
        .join("styles")
        .join(format!("{}.css", exe_name));

    // If the file doesn't exist or can't be read, `ok()` will gracefully convert the error
    // to `None`, which will then be returned by the function.
    let css_content = std::fs::read_to_string(css_path).ok()?;

    if css_content.trim().is_empty() {
        return None;
    }

    let (mut css, _warnings) = new_from_str(&css_content);

    // The parser returns a `Css` which contains a `Vec<Stylesheet>`.
    // For an app-specific theme file, we are only interested in the first stylesheet.
    if !css.stylesheets.is_empty() {
        let mut owned_vec = css.stylesheets.into_library_owned_vec();
        Some(owned_vec.remove(0))
    } else {
        None
    }
}

pub mod defaults {
    //! A collection of hard-coded system style defaults that mimic the appearance
    //! of various operating systems and desktop environments. These are used as a
    //! fallback when the "io" feature is disabled, ensuring deterministic styles
    //! for testing and environments where system calls are not desired.

    use crate::{
        props::{
            basic::{color::ColorU, pixel::PixelValue},
            layout::{
                dimensions::LayoutWidth,
                spacing::{LayoutPaddingLeft, LayoutPaddingRight},
            },
            style::{
                background::StyleBackgroundContent,
                scrollbar::{
                    ComputedScrollbarStyle, ScrollbarInfo, SCROLLBAR_ANDROID_DARK,
                    SCROLLBAR_ANDROID_LIGHT, SCROLLBAR_CLASSIC_DARK, SCROLLBAR_CLASSIC_LIGHT,
                    SCROLLBAR_IOS_DARK, SCROLLBAR_IOS_LIGHT, SCROLLBAR_MACOS_DARK,
                    SCROLLBAR_MACOS_LIGHT, SCROLLBAR_WINDOWS_DARK, SCROLLBAR_WINDOWS_LIGHT,
                },
            },
        },
        system::{
            DesktopEnvironment, Platform, SystemColors, SystemFonts, SystemMetrics, SystemStyle,
            Theme,
        },
    };

    // --- Custom Scrollbar Style Constants for Nostalgia ---

    /// A scrollbar style mimicking the classic Windows 95/98/2000/XP look.
    pub const SCROLLBAR_WINDOWS_CLASSIC: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(17)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 223,
            g: 223,
            b: 223,
            a: 255,
        }), // Scrollbar trough color
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 208,
            g: 208,
            b: 208,
            a: 255,
        }), // Button face color
        button: StyleBackgroundContent::Color(ColorU {
            r: 208,
            g: 208,
            b: 208,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU {
            r: 223,
            g: 223,
            b: 223,
            a: 255,
        }),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    };

    /// A scrollbar style mimicking the macOS "Aqua" theme from the early 2000s.
    pub const SCROLLBAR_MACOS_AQUA: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(15)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 238,
            g: 238,
            b: 238,
            a: 128,
        }), // Translucent track
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 105,
            g: 173,
            b: 255,
            a: 255,
        }), // "Gel" blue
        button: StyleBackgroundContent::Color(ColorU {
            r: 105,
            g: 173,
            b: 255,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    };

    /// A scrollbar style mimicking the KDE Oxygen theme.
    pub const SCROLLBAR_KDE_OXYGEN: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(14)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(2),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(2),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 242,
            g: 242,
            b: 242,
            a: 255,
        }),
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 177,
            g: 177,
            b: 177,
            a: 255,
        }),
        button: StyleBackgroundContent::Color(ColorU {
            r: 216,
            g: 216,
            b: 216,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    };

    /// Helper to convert a detailed `ScrollbarInfo` into the simplified `ComputedScrollbarStyle`.
    fn scrollbar_info_to_computed(info: &ScrollbarInfo) -> ComputedScrollbarStyle {
        ComputedScrollbarStyle {
            width: Some(info.width),
            thumb_color: match info.thumb {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
            track_color: match info.track {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
        }
    }

    // --- Windows Styles ---

    pub fn windows_11_light() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(243, 243, 243)),
                accent: Some(ColorU::new_rgb(0, 95, 184)),
                window_background: Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: Some(ColorU::new_rgb(0, 120, 215)),
                selection_text: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Segoe UI Variable Text".into()),
                ui_font_size: Some(9.0),
                monospace_font: Some("Consolas".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn windows_11_dark() -> SystemStyle {
        SystemStyle {
            theme: Theme::Dark,
            platform: Platform::Windows,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(255, 255, 255)),
                background: Some(ColorU::new_rgb(32, 32, 32)),
                accent: Some(ColorU::new_rgb(0, 120, 215)),
                window_background: Some(ColorU::new_rgb(25, 25, 25)),
                selection_background: Some(ColorU::new_rgb(0, 120, 215)),
                selection_text: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Segoe UI Variable Text".into()),
                ui_font_size: Some(9.0),
                monospace_font: Some("Consolas".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_DARK)),
            app_specific_stylesheet: None,
        }
    }

    pub fn windows_7_aero() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(240, 240, 240)),
                accent: Some(ColorU::new_rgb(51, 153, 255)),
                window_background: Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: Some(ColorU::new_rgb(51, 153, 255)),
                selection_text: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Segoe UI".into()),
                ui_font_size: Some(9.0),
                monospace_font: Some("Consolas".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(6.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn windows_xp_luna() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(236, 233, 216)),
                accent: Some(ColorU::new_rgb(49, 106, 197)),
                window_background: Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: Some(ColorU::new_rgb(49, 106, 197)),
                selection_text: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Tahoma".into()),
                ui_font_size: Some(8.0),
                monospace_font: Some("Lucida Console".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(3.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_CLASSIC)),
            app_specific_stylesheet: None,
        }
    }

    // --- macOS Styles ---

    pub fn macos_modern_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new(0, 0, 0, 221)),
                background: Some(ColorU::new_rgb(242, 242, 247)),
                accent: Some(ColorU::new_rgb(0, 122, 255)),
                window_background: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some(".SF NS".into()),
                ui_font_size: Some(13.0),
                monospace_font: Some("Menlo".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(8.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_MACOS_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn macos_modern_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Dark,
            colors: SystemColors {
                text: Some(ColorU::new(255, 255, 255, 221)),
                background: Some(ColorU::new_rgb(28, 28, 30)),
                accent: Some(ColorU::new_rgb(10, 132, 255)),
                window_background: Some(ColorU::new_rgb(44, 44, 46)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some(".SF NS".into()),
                ui_font_size: Some(13.0),
                monospace_font: Some("Menlo".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(8.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_MACOS_DARK)),
            app_specific_stylesheet: None,
        }
    }

    pub fn macos_aqua() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(229, 229, 229)),
                accent: Some(ColorU::new_rgb(63, 128, 234)),
                window_background: Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Lucida Grande".into()),
                ui_font_size: Some(13.0),
                monospace_font: Some("Monaco".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(12.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_MACOS_AQUA)),
            app_specific_stylesheet: None,
        }
    }

    // --- Linux Styles ---

    pub fn gnome_adwaita_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(46, 52, 54)),
                background: Some(ColorU::new_rgb(249, 249, 249)),
                accent: Some(ColorU::new_rgb(53, 132, 228)),
                window_background: Some(ColorU::new_rgb(237, 237, 237)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Cantarell".into()),
                ui_font_size: Some(11.0),
                monospace_font: Some("Monospace".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn gnome_adwaita_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Dark,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(238, 238, 236)),
                background: Some(ColorU::new_rgb(36, 36, 36)),
                accent: Some(ColorU::new_rgb(53, 132, 228)),
                window_background: Some(ColorU::new_rgb(48, 48, 48)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Cantarell".into()),
                ui_font_size: Some(11.0),
                monospace_font: Some("Monospace".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_DARK)),
            app_specific_stylesheet: None,
        }
    }

    pub fn gtk2_clearlooks() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(239, 239, 239)),
                accent: Some(ColorU::new_rgb(245, 121, 0)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("DejaVu Sans".into()),
                ui_font_size: Some(10.0),
                monospace_font: Some("DejaVu Sans Mono".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn kde_breeze_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Kde),
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(31, 36, 39)),
                background: Some(ColorU::new_rgb(239, 240, 241)),
                accent: Some(ColorU::new_rgb(61, 174, 233)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Noto Sans".into()),
                ui_font_size: Some(10.0),
                monospace_font: Some("Hack".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(4.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_KDE_OXYGEN)),
            app_specific_stylesheet: None,
        }
    }

    // --- Mobile Styles ---

    pub fn android_material_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Android,
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(255, 255, 255)),
                accent: Some(ColorU::new_rgb(98, 0, 238)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Roboto".into()),
                ui_font_size: Some(14.0),
                monospace_font: Some("Droid Sans Mono".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(12.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_ANDROID_LIGHT)),
            app_specific_stylesheet: None,
        }
    }

    pub fn android_holo_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::Android,
            theme: Theme::Dark,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(255, 255, 255)),
                background: Some(ColorU::new_rgb(0, 0, 0)),
                accent: Some(ColorU::new_rgb(51, 181, 229)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some("Roboto".into()),
                ui_font_size: Some(14.0),
                monospace_font: Some("Droid Sans Mono".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(2.0)),
                border_width: Some(PixelValue::px(1.0)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_ANDROID_DARK)),
            app_specific_stylesheet: None,
        }
    }

    pub fn ios_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Ios,
            theme: Theme::Light,
            colors: SystemColors {
                text: Some(ColorU::new_rgb(0, 0, 0)),
                background: Some(ColorU::new_rgb(242, 242, 247)),
                accent: Some(ColorU::new_rgb(0, 122, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: Some(".SFUI-Display-Regular".into()),
                ui_font_size: Some(17.0),
                monospace_font: Some("Menlo".into()),
            },
            metrics: SystemMetrics {
                corner_radius: Some(PixelValue::px(10.0)),
                border_width: Some(PixelValue::px(0.5)),
            },
            scrollbar: Some(scrollbar_info_to_computed(&SCROLLBAR_IOS_LIGHT)),
            app_specific_stylesheet: None,
        }
    }
}
