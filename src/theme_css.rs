use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

pub const TOKENS: &[(&str, &str)] = &[
    ("bg-canvas", "bgCanvas"),
    ("bg-panel", "bgPanel"),
    ("bg-surface", "bgSurface"),
    ("bg-menu", "bgMenu"),
    ("bg-raised", "bgRaised"),
    ("bg-inset", "bgInset"),
    ("bg-hover", "bgHover"),
    ("bg-subtle", "bgSubtle"),
    ("border-strong", "borderStrong"),
    ("border-hairline", "borderHairline"),
    ("border-panel", "borderPanel"),
    ("grid-dot", "gridDot"),
    ("text-primary", "textPrimary"),
    ("text-strong", "textStrong"),
    ("text-secondary", "textSecondary"),
    ("text-icon", "textIcon"),
    ("text-muted", "textMuted"),
    ("text-disabled", "textDisabled"),
    ("text-on-accent", "textOnAccent"),
    ("text-destructive", "textDestructive"),
    ("accent", "accent"),
    ("accent-hover", "accentHover"),
    ("sig-audio", "sigAudio"),
    ("sig-midi", "sigMidi"),
    ("sig-video", "sigVideo"),
    ("cat-hardware", "catHardware"),
    ("cat-virtual", "catVirtual"),
    ("cat-app", "catApp"),
    ("cat-plugin", "catPlugin"),
    ("cat-midi", "catMidi"),
    ("cat-subgraph", "catSubgraph"),
];

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
pub struct CustomTheme {
    pub dark: BTreeMap<String, String>,
    pub light: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgba {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

impl Rgba {
    fn hex(self) -> String {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        if c(self.a) == 255 {
            format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", c(self.a), c(self.r), c(self.g), c(self.b))
        }
    }

    fn mix(self, other: Rgba, t: f64) -> Rgba {
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: 1.0,
        }
    }

    fn luminance(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Both,
    Dark,
    Light,
}

pub fn themes_dir() -> PathBuf {
    crate::session::root_dir().join("themes")
}

pub fn valid_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

pub fn list_files() -> Vec<(String, PathBuf, Option<SystemTime>, u64)> {
    let Ok(entries) = std::fs::read_dir(themes_dir()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, Option<SystemTime>, u64)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("css")))
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            if !valid_theme_name(&name) {
                return None;
            }
            let meta = std::fs::metadata(&p).ok();
            let modified = meta.as_ref().and_then(|m| m.modified().ok());
            let len = meta.map_or(0, |m| m.len());
            Some((name, p, modified, len))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn load_all() -> Vec<(String, CustomTheme)> {
    list_files()
        .into_iter()
        .filter_map(|(name, path, _, len)| {
            if len > 256 * 1024 {
                log::warn!("Theme {} is too large; skipped", path.display());
                return None;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => Some((name, parse(&text))),
                Err(e) => {
                    log::warn!("Could not read theme {}: {}", path.display(), e);
                    None
                }
            }
        })
        .collect()
}

pub fn parse(css: &str) -> CustomTheme {
    let css = strip_comments(css);
    let mut both = BTreeMap::new();
    let mut dark = BTreeMap::new();
    let mut light = BTreeMap::new();
    collect(&css, Scope::Both, &mut both, &mut dark, &mut light);

    let mut out = CustomTheme::default();
    for (scope_vars, target) in [(&dark, &mut out.dark), (&light, &mut out.light)] {
        let mut vars = both.clone();
        vars.extend(scope_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
        *target = build_palette(&vars);
    }
    out
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn scope_of(prelude: &str, outer: Scope) -> Scope {
    let p = prelude.to_ascii_lowercase().replace([' ', '"', '\''], "");
    if p.contains("prefers-color-scheme:dark") || p.contains(".dark") || p.contains("data-theme=dark") {
        Scope::Dark
    } else if p.contains("prefers-color-scheme:light")
        || p.contains(".light")
        || p.contains("data-theme=light")
    {
        Scope::Light
    } else {
        outer
    }
}

fn collect(
    css: &str,
    scope: Scope,
    both: &mut BTreeMap<String, String>,
    dark: &mut BTreeMap<String, String>,
    light: &mut BTreeMap<String, String>,
) {
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut prelude_start = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let prelude = css[prelude_start..i].trim();
                let mut depth = 1;
                let body_start = i + 1;
                let mut j = body_start;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let body_end = if depth == 0 { j - 1 } else { bytes.len() };
                let body = &css[body_start..body_end];
                let inner = scope_of(prelude, scope);
                if body.contains('{') {
                    collect(body, inner, both, dark, light);
                } else {
                    let target = match inner {
                        Scope::Both => &mut *both,
                        Scope::Dark => &mut *dark,
                        Scope::Light => &mut *light,
                    };
                    for decl in body.split(';') {
                        if let Some((name, value)) = decl.split_once(':') {
                            let name = name.trim();
                            if let Some(var) = name.strip_prefix("--") {
                                target.insert(var.to_ascii_lowercase(), value.trim().to_string());
                            }
                        }
                    }
                }
                i = j;
                prelude_start = j;
            }
            b';' | b'}' => {
                i += 1;
                prelude_start = i;
            }
            _ => i += 1,
        }
    }
}

fn resolve(vars: &BTreeMap<String, String>, name: &str, depth: usize) -> Option<Rgba> {
    if depth > 8 {
        return None;
    }
    let value = vars.get(name)?;
    value_color(vars, value, depth)
}

fn value_color(vars: &BTreeMap<String, String>, value: &str, depth: usize) -> Option<Rgba> {
    let v = value.trim().trim_end_matches("!important").trim();
    if let Some(inner) = v.strip_prefix("var(").and_then(|s| s.strip_suffix(')')) {
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner.trim(), None),
        };
        let name = name.strip_prefix("--")?.to_ascii_lowercase();
        return resolve(vars, &name, depth + 1)
            .or_else(|| fallback.and_then(|f| value_color(vars, f, depth + 1)));
    }
    parse_color(v)
}

fn parse_color(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = v.to_ascii_lowercase();
    let args = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = args
        .split([',', ' ', '/'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let channel = |s: &str| -> Option<f64> {
        match s.strip_suffix('%') {
            Some(p) => p.parse::<f64>().ok().map(|x| x / 100.0),
            None => s.parse::<f64>().ok().map(|x| x / 255.0),
        }
    };
    let alpha = match parts.get(3) {
        Some(s) => match s.strip_suffix('%') {
            Some(p) => p.parse::<f64>().ok()? / 100.0,
            None => s.parse::<f64>().ok()?,
        },
        None => 1.0,
    };
    Some(Rgba {
        r: channel(parts[0])?,
        g: channel(parts[1])?,
        b: channel(parts[2])?,
        a: alpha.clamp(0.0, 1.0),
    })
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digits: Vec<f64> = match hex.len() {
        3 | 4 => hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| (d * 17) as f64 / 255.0))
            .collect::<Option<_>>()?,
        6 | 8 => (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok().map(|d| d as f64 / 255.0))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    Some(Rgba {
        r: digits[0],
        g: digits[1],
        b: digits[2],
        a: digits.get(3).copied().unwrap_or(1.0),
    })
}

struct Scheme {
    bg: Rgba,
    fg: Rgba,
    red: Rgba,
    orange: Rgba,
    yellow: Rgba,
    green: Rgba,
    cyan: Rgba,
    blue: Rgba,
    magenta: Rgba,
}

fn scheme(vars: &BTreeMap<String, String>) -> Option<Scheme> {
    let get = |name: &str| resolve(vars, name, 0);
    if let (Some(bg), Some(fg)) = (get("base00"), get("base05")) {
        let any = |name: &str, fallback: Rgba| get(name).unwrap_or(fallback);
        let red = any("base08", fg);
        return Some(Scheme {
            bg,
            fg,
            red,
            orange: any("base09", red),
            yellow: any("base0a", fg),
            green: any("base0b", fg),
            cyan: any("base0c", fg),
            blue: any("base0d", fg),
            magenta: any("base0e", fg),
        });
    }
    let bg = get("background").or_else(|| get("color0"))?;
    let fg = get("foreground").or_else(|| get("color7")).or_else(|| get("color15"))?;
    let any = |name: &str| get(name).unwrap_or(fg);
    let red = any("color1");
    let yellow = any("color3");
    Some(Scheme {
        bg,
        fg,
        red,
        orange: red.mix(yellow, 0.5),
        yellow,
        green: any("color2"),
        cyan: any("color6"),
        blue: any("color4"),
        magenta: any("color5"),
    })
}

fn derive(s: &Scheme) -> BTreeMap<&'static str, Rgba> {
    let dark = s.bg.luminance() < 0.5;
    let extreme = if dark {
        Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }
    } else {
        Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }
    };
    let m = |t: f64| s.bg.mix(s.fg, t);
    BTreeMap::from([
        ("bgCanvas", s.bg),
        ("bgPanel", m(0.03)),
        ("bgSurface", m(0.06)),
        ("bgMenu", m(0.08)),
        ("bgRaised", m(0.08)),
        ("bgInset", s.bg),
        ("bgHover", m(0.10)),
        ("bgSubtle", m(0.12)),
        ("borderStrong", m(0.18)),
        ("borderHairline", m(0.12)),
        ("borderPanel", m(0.10)),
        ("gridDot", m(0.24)),
        ("textPrimary", s.fg),
        ("textStrong", s.fg.mix(extreme, 0.3)),
        ("textSecondary", m(0.72)),
        ("textIcon", m(0.85)),
        ("textMuted", m(0.58)),
        ("textDisabled", m(0.40)),
        ("textOnAccent", s.bg),
        ("textDestructive", s.red),
        ("accent", s.blue),
        ("accentHover", s.blue.mix(s.fg, 0.25)),
        ("sigAudio", s.cyan),
        ("sigMidi", s.magenta),
        ("sigVideo", s.orange),
        ("catHardware", s.cyan),
        ("catVirtual", s.blue),
        ("catApp", s.orange),
        ("catPlugin", s.yellow),
        ("catMidi", s.magenta),
        ("catSubgraph", s.green),
    ])
}

fn build_palette(vars: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    if let Some(s) = scheme(vars) {
        for (token, color) in derive(&s) {
            out.insert(token.to_string(), color.hex());
        }
    }
    for (css_name, token) in TOKENS {
        if let Some(color) = resolve(vars, css_name, 0) {
            out.insert(token.to_string(), color.hex());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_tokens_apply_to_both_modes() {
        let t = parse(":root { --accent: #cba6f7; --bg-canvas: #11111b; --nonsense: #fff; }");
        assert_eq!(t.dark.get("accent").map(String::as_str), Some("#cba6f7"));
        assert_eq!(t.light.get("bgCanvas").map(String::as_str), Some("#11111b"));
        assert_eq!(t.dark.len(), 2);
    }

    #[test]
    fn media_and_class_blocks_set_one_mode() {
        let css = r#"
            /* comment { --accent: #000000; } */
            :root { --accent: #111111; }
            @media (prefers-color-scheme: dark) { :root { --accent: #222222; } }
            :root[data-theme="light"] { --accent: #333333 }
        "#;
        let t = parse(css);
        assert_eq!(t.dark["accent"], "#222222");
        assert_eq!(t.light["accent"], "#333333");
    }

    #[test]
    fn colour_formats_and_var_references() {
        let css = ":root { --base: rgb(255, 0, 0); --accent: var(--base); --sig-midi: #0f08; \
                   --sig-audio: rgba(0 0 255 / 50%); --text-primary: var(--missing, #abc); --cat-app: nope; }";
        let t = parse(css);
        assert_eq!(t.dark["accent"], "#ff0000");
        assert_eq!(t.dark["sigMidi"], "#8800ff00");
        assert_eq!(t.dark["sigAudio"], "#800000ff");
        assert_eq!(t.dark["textPrimary"], "#aabbcc");
        assert!(!t.dark.contains_key("catApp"));
    }

    #[test]
    fn pywal_colours_derive_a_full_palette() {
        let mut css = String::from(":root { --background: #1d1f21; --foreground: #c5c8c6; --cursor: #c5c8c6;");
        for (i, c) in ["#1d1f21", "#cc6666", "#b5bd68", "#f0c674", "#81a2be", "#b294bb", "#8abeb7", "#c5c8c6"]
            .iter()
            .enumerate()
        {
            css.push_str(&format!(" --color{i}: {c};"));
        }
        css.push_str(" --accent: #ff00ff; }");
        let t = parse(&css);
        assert_eq!(t.dark.len(), TOKENS.len());
        assert_eq!(t.dark["bgCanvas"], "#1d1f21");
        assert_eq!(t.dark["textPrimary"], "#c5c8c6");
        assert_eq!(t.dark["textDestructive"], "#cc6666");
        assert_eq!(t.dark["accent"], "#ff00ff");
        assert_ne!(t.dark["bgSurface"], t.dark["bgCanvas"]);
        assert_ne!(t.dark["catApp"], t.dark["catPlugin"]);
    }

    #[test]
    fn base16_colours_derive_a_full_palette() {
        let css = ":root { --base00: #2e3440; --base05: #e5e9f0; --base08: #bf616a; --base0D: #81a1c1; }";
        let t = parse(css);
        assert_eq!(t.light.len(), TOKENS.len());
        assert_eq!(t.light["accent"], "#81a1c1");
        assert_eq!(t.light["sigVideo"], "#bf616a");
    }

    #[test]
    fn broken_css_never_panics() {
        for css in ["", "{", "}", ":root {", ":root { --accent }", "@media { :root { --accent: #12", "/* open"] {
            let _ = parse(css);
        }
    }

    #[test]
    fn theme_names_must_be_plain() {
        assert!(valid_theme_name("wal"));
        assert!(!valid_theme_name(".hidden"));
        assert!(!valid_theme_name("a/b"));
    }
}
