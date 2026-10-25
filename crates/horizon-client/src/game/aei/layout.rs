//! Native geometry following JEI 19's IngredientListOverlay and RecipesGui.
use horizon_client::hud::{HudAtlas, HudBatch, rgba};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }
    pub fn fill(self, batch: &mut HudBatch, atlas: &HudAtlas, color: u32) {
        batch.fill(atlas, self.x, self.y, self.w, self.h, color);
    }
}

pub(super) struct Catalogue {
    pub grid: Rect,
    pub search: Rect,
    pub navigation: Rect,
    pub categories: Rect,
    pub columns: usize,
    pub rows: usize,
}
impl Catalogue {
    pub fn new(width: f32, height: f32, gui_right: f32) -> Self {
        let columns = (((width - gui_right - 12.0) / 18.0).floor().max(0.0) as usize).min(9);
        let rows = (((height - 56.0) / 18.0).floor().max(0.0) as usize).min(16);
        let w = columns as f32 * 18.0;
        // Keep the complete overlay against the screen's six-pixel margin.
        let x = width - 6.0 - w;
        Self {
            grid: Rect::new(x, 28.0, w, rows as f32 * 18.0),
            navigation: Rect::new(x, 6.0, w, 20.0),
            search: Rect::new(x, height - 26.0, (w - 22.0).max(0.0), 20.0),
            categories: Rect::new(x + (w - 20.0).max(0.0), height - 26.0, w.min(20.0), 20.0),
            columns,
            rows,
        }
    }
    pub fn page_size(&self) -> usize {
        self.columns * self.rows
    }
    pub fn pages(&self, count: usize) -> usize {
        count.div_ceil(self.page_size().max(1)).max(1)
    }
    pub fn item_at(&self, p: (f32, f32), page: usize) -> Option<usize> {
        if !self.grid.contains(p) {
            return None;
        }
        let col = ((p.0 - self.grid.x) / 18.0) as usize;
        let row = ((p.1 - self.grid.y) / 18.0) as usize;
        Some(page * self.page_size() + row * self.columns + col)
    }
    pub fn slot(&self, i: usize) -> Rect {
        Rect::new(
            self.grid.x + (i % self.columns) as f32 * 18.0,
            self.grid.y + (i / self.columns) as f32 * 18.0,
            18.0,
            18.0,
        )
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        self.grid.contains(p)
            || self.navigation.contains(p)
            || self.search.contains(p)
            || self.categories.contains(p)
    }
}

pub(super) struct Recipes {
    pub area: Rect,
    pub per_page: usize,
    recipe_width: f32,
    recipe_height: f32,
}
impl Recipes {
    pub fn new(width: f32, height: f32) -> Self {
        let available = (height - 58.0).max(175.0);
        let h = available.min(350.0);
        let y = 45.0 + ((available - h) / 2.0).floor();
        Self {
            area: Rect::new(((width - 198.0) / 2.0).floor(), y, 198.0, h),
            recipe_width: 124.0,
            recipe_height: 62.0,
            per_page: (1.0 + ((h - 40.0 - 62.0) / 66.0).trunc()).max(1.0) as usize,
        }
    }
    pub fn with_recipe_size(mut self, screen_width: f32, width: f32, height: f32) -> Self {
        self.recipe_width = width;
        self.recipe_height = height;
        self.area.w = 198.0_f32.max(width + 12.0);
        self.area.x = ((screen_width - self.area.w) / 2.0).floor();
        self.per_page =
            (1.0 + ((self.area.h - 40.0 - height) / (height + 4.0)).trunc()).max(1.0) as usize;
        self
    }
    pub fn recipe(&self, i: usize) -> Rect {
        let gap = (((self.area.h - 40.0).max(self.recipe_height)
            - self.per_page as f32 * self.recipe_height)
            / (self.per_page + 1) as f32)
            .floor();
        Rect::new(
            self.area.x + ((self.area.w - self.recipe_width) / 2.0).floor(),
            self.area.y + 34.0 + gap + i as f32 * (self.recipe_height + gap),
            self.recipe_width,
            self.recipe_height,
        )
    }
    pub fn tab(&self, i: usize) -> Rect {
        Rect::new(
            self.area.x + 2.0 + i as f32 * 24.0,
            self.area.y - 21.0,
            24.0,
            24.0,
        )
    }
    pub fn nav(&self, categories: bool) -> Rect {
        Rect::new(
            self.area.x + 6.0,
            self.area.y + if categories { 4.0 } else { 19.0 },
            self.area.w - 12.0,
            13.0,
        )
    }
}

/// Draw a portion of a bundled JEI sprite without resampling its pixel grid.
pub(super) fn part(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    name: &str,
    src: Rect,
    dst: Rect,
    color: u32,
) {
    let Some(mut uv) = atlas.sprites.get(&format!("horizon/aei/{name}")).copied() else {
        return;
    };
    let (du, dv) = ((uv.u1 - uv.u0) / uv.w as f32, (uv.v1 - uv.v0) / uv.h as f32);
    uv.u0 += src.x * du;
    uv.v0 += src.y * dv;
    uv.u1 = uv.u0 + src.w * du;
    uv.v1 = uv.v0 + src.h * dv;
    batch.rect_uv(dst.x, dst.y, dst.w, dst.h, uv, color);
}
/// DrawableNineSliceTexture tiles from the left and bottom, cropping the
/// topmost partial tile from its bottom edge (not stretching the center).
pub(super) fn nine_slice(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    name: &str,
    dst: Rect,
    [l, r, t, b]: [f32; 4],
) {
    let Some(image) = atlas.sprites.get(&format!("horizon/aei/{name}")) else {
        return;
    };
    let (w, h) = (image.w as f32, image.h as f32);
    for (sy, sh, dy, dh) in [
        (0.0, t, dst.y, t),
        (t, h - t - b, dst.y + t, dst.h - t - b),
        (h - b, b, dst.y + dst.h - b, b),
    ] {
        for (sx, sw, dx, dw) in [
            (0.0, l, dst.x, l),
            (l, w - l - r, dst.x + l, dst.w - l - r),
            (w - r, r, dst.x + dst.w - r, r),
        ] {
            let mut x = dx;
            while x < dx + dw {
                let tw = sw.min(dx + dw - x);
                let mut bottom = dy + dh;
                while bottom > dy {
                    let th = sh.min(bottom - dy);
                    part(
                        batch,
                        atlas,
                        name,
                        Rect::new(sx, sy + sh - th, tw, th),
                        Rect::new(x, bottom - th, tw, th),
                        u32::MAX,
                    );
                    bottom -= th;
                }
                x += tw;
            }
        }
    }
}

pub(super) fn panel(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, inset: bool) {
    if inset {
        let name = if r.w == 26.0 { "output_slot" } else { "slot" };
        batch.sprite(atlas, &format!("horizon/aei/{name}"), r.x, r.y);
    } else {
        nine_slice(batch, atlas, "single_recipe_background", r, [16.0; 4]);
    }
}

pub(super) fn nav_buttons(r: Rect) -> [Rect; 2] {
    let w = r.h.min((r.w / 2.0).floor());
    [
        Rect::new(r.x, r.y, w, r.h),
        Rect::new(r.x + r.w - w, r.y, w, r.h),
    ]
}
pub(super) fn navigation(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    r: Rect,
    label: &str,
    enabled: bool,
    cursor: (f32, f32),
    pressed: Option<Rect>,
) {
    let buttons = nav_buttons(r);
    Rect::new(
        r.x + buttons[0].w,
        r.y,
        r.w - buttons[0].w - buttons[1].w,
        r.h,
    )
    .fill(batch, atlas, rgba(0, 0, 0, 48));
    let tw = HudBatch::text_width(atlas, label);
    if tw <= r.w - buttons[0].w - buttons[1].w {
        batch.text(
            atlas,
            label,
            r.x + ((r.w - tw) / 2.0).round(),
            r.y + ((r.h - 9.0) / 2.0).round(),
            u32::MAX,
            true,
        );
    }
    for (i, b) in buttons.into_iter().enumerate() {
        let hovered = b.contains(cursor);
        let down = enabled && pressed == Some(b);
        let texture = if !enabled {
            "button_disabled"
        } else if down {
            if hovered {
                "button_pressed_highlight"
            } else {
                "button_pressed"
            }
        } else if hovered {
            "button_highlight"
        } else {
            "button_enabled"
        };
        nine_slice(batch, atlas, texture, b, [6.0; 4]);
        let tint = if !enabled {
            160
        } else if hovered {
            255
        } else {
            224
        };
        let offset = if down { 0.5 } else { 0.0 };
        let x = b.x + ((b.w - 9.0) / 2.0) + offset;
        let y = b.y + ((b.h - 9.0) / 2.0) + offset;
        // DrawableSprite.trim(0, 0, 1, 1) keeps a 9x9 layout but omits
        // the top/bottom rows, including the bottom shadow pixel.
        let icon = if i == 0 {
            "icons/arrow_previous"
        } else {
            "icons/arrow_next"
        };
        part(
            batch,
            atlas,
            icon,
            Rect::new(0.0, 1.0, 9.0, 7.0),
            Rect::new(x, y + 1.0, 9.0, 7.0),
            rgba(tint, tint, tint, 255),
        );
    }
}

/// CraftingGridHelper.getCraftingIndex: JEI centers one-wide/one-high
/// recipes, lays 2-wide recipes into two columns, and lowers a 3x2 recipe.
pub(super) fn crafting_index(i: usize, width: usize, height: usize) -> usize {
    if width == 1 {
        if height == 1 { 4 } else { i * 3 + 1 }
    } else if height == 1 {
        i + 3
    } else if width == 2 {
        i + i / 2
    } else if height == 2 {
        i + 3
    } else {
        i
    }
}

pub(super) fn change_page(page: usize, pages: usize, backwards: bool) -> usize {
    if backwards {
        (page.min(pages - 1) + pages - 1) % pages
    } else {
        (page.min(pages - 1) + 1) % pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidebar_fits_small_and_wide_gui_without_covering_inventory() {
        for (w, h) in [
            (320.0, 240.0),
            (427.0, 240.0),
            (640.0, 360.0),
            (1920.0, 1080.0),
        ] {
            let right = (w + 176.0) / 2.0;
            let c = Catalogue::new(w, h, right);
            assert!(c.columns > 0 && c.rows > 0);
            assert!(c.grid.x >= right + 6.0);
            assert_eq!(c.grid.x + c.grid.w, w - 6.0);
            assert_eq!(c.categories.x + c.categories.w, w - 6.0);
            assert!(c.search.x + c.search.w <= c.categories.x);
            assert!(c.grid.y + c.grid.h <= c.search.y - 2.0);
            assert_eq!(c.item_at((c.grid.x, c.grid.y), 1), Some(c.page_size()));
            assert_eq!(c.item_at((c.grid.x + c.grid.w, c.grid.y), 0), None);
        }
    }
    #[test]
    fn jei_crafting_positions_center_and_preserve_recipe_shape() {
        assert_eq!(crafting_index(0, 1, 1), 4);
        assert_eq!(
            (0..2).map(|i| crafting_index(i, 1, 2)).collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(
            (0..4).map(|i| crafting_index(i, 2, 2)).collect::<Vec<_>>(),
            vec![0, 1, 3, 4]
        );
        assert_eq!(
            (0..6).map(|i| crafting_index(i, 3, 2)).collect::<Vec<_>>(),
            vec![3, 4, 5, 6, 7, 8]
        );
        for width in 1..=3 {
            for height in 1..=3 {
                let indices = (0..width * height)
                    .map(|i| crafting_index(i, width, height))
                    .collect::<std::collections::HashSet<_>>();
                assert_eq!(indices.len(), width * height);
                assert!(indices.iter().all(|i| *i < 9));
            }
        }
    }
    #[test]
    fn recipe_geometry_matches_jei_default_and_height_cap() {
        let r = Recipes::new(640.0, 360.0);
        assert_eq!(r.area, Rect::new(221.0, 45.0, 198.0, 302.0));
        assert_eq!(r.per_page, 4);
        assert_eq!(r.recipe(0), Rect::new(258.0, 81.0, 124.0, 62.0));
        assert_eq!(r.tab(0), Rect::new(223.0, 24.0, 24.0, 24.0));
        assert_eq!(
            nav_buttons(r.nav(true)),
            [
                Rect::new(227.0, 49.0, 13.0, 13.0),
                Rect::new(400.0, 49.0, 13.0, 13.0)
            ]
        );
        assert_eq!(
            nav_buttons(r.nav(false))[0],
            Rect::new(227.0, 64.0, 13.0, 13.0)
        );
        let tall = Recipes::new(1920.0, 1080.0);
        assert_eq!(tall.area.h, 350.0);
        assert_eq!(tall.area.y, 381.0);
        assert_eq!(Recipes::new(320.0, 240.0).per_page, 2);
        assert_eq!(nav_buttons(Rect::new(0.0, 0.0, 18.0, 20.0))[0].w, 9.0);
    }
    #[test]
    fn pages_remain_valid_after_search_or_resize() {
        let c = Catalogue::new(427.0, 240.0, 301.0);
        assert_eq!(c.pages(0), 1);
        assert_eq!(change_page(20, c.pages(1), false), 0);
        assert_eq!(change_page(0, 3, true), 2);
        let r = Recipes::new(427.0, 240.0);
        let last = r.recipe(r.per_page - 1);
        assert!(last.y + last.h <= r.area.y + r.area.h);
    }
}
