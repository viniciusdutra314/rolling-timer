use crate::preferences::Theme;
use fltk::enums::Color;

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub accent: Color,
    pub stop: Color,
    pub button_text: Color,
    pub grid: Color,
}
impl Theme {
    pub(super) fn palette(self) -> Palette {
        // These are our own small pastel palettes, not downloaded theme packs.
        let (background, surface, text, accent, stop, button_text, grid) = match self {
            Self::Dark => (
                (32, 38, 32),
                (48, 59, 50),
                (242, 246, 241),
                (101, 223, 139),
                (235, 141, 131),
                (24, 30, 25),
                (104, 120, 109),
            ),
            Self::Light => (
                (246, 248, 250),
                (255, 255, 255),
                (30, 38, 48),
                (38, 112, 77),
                (160, 53, 53),
                (255, 255, 255),
                (173, 185, 195),
            ),
            Self::Mocha => (
                (35, 31, 46),
                (54, 47, 66),
                (240, 222, 239),
                (191, 174, 231),
                (237, 167, 180),
                (37, 29, 48),
                (126, 111, 143),
            ),
            Self::Cappuccino => (
                (247, 237, 218),
                (255, 249, 237),
                (73, 54, 44),
                (168, 198, 167),
                (224, 165, 145),
                (52, 40, 33),
                (171, 148, 122),
            ),
        };
        let color = |(r, g, b)| Color::from_rgb(r, g, b);
        Palette {
            background: color(background),
            surface: color(surface),
            text: color(text),
            accent: color(accent),
            stop: color(stop),
            button_text: color(button_text),
            grid: color(grid),
        }
    }
}
