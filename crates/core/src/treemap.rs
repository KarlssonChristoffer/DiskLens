//! Cushion treemap layout and rendering.
//!
//! A port of WinDirStat 2.5.0's CTreeMap (Controls/TreeMap.cpp): KDirStat-style
//! row layout or SequoiaView-style squarification, then van Wijk & van de
//! Wetering's cushion shading. Each nested rectangle adds a parabolic "ridge"
//! to a surface `z = s0*x² + s1*y² + s2*x + s3*y`, and every pixel is lit by
//! the angle between that surface's normal and a light source.
//!
//! Layout runs on one thread; shading, the expensive part, runs on the rayon
//! pool in horizontal bands so every thread owns its pixels.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::palette::{PALETTE_BRIGHTNESS, Rgb, make_bright, normalize};
use crate::tree::{NO_EXT, NodeId, NodeKind, SizeMode, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    /// Children laid out in rows, as KDirStat does. WinDirStat's default.
    KDirStat,
    /// Classic squarified treemap (Bruls, Huizing, van Wijk).
    Squarified,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub style: Style,
    /// Leave a one-pixel gap (in `grid_color`) between rectangles.
    pub grid: bool,
    pub grid_color: Rgb,
    /// 0..1, overall brightness.
    pub brightness: f64,
    /// Cushion height factor "H".
    pub height: f64,
    /// Ridge scale factor "F" applied per nesting level.
    pub scale_factor: f64,
    /// Ambient light "Ia", 0..1.
    pub ambient_light: f64,
    /// Light source direction; negative x is left, negative y is top.
    pub light_x: f64,
    pub light_y: f64,
}

impl Default for Options {
    /// WinDirStat 2.5.0's defaults (TreeMap.h `DefaultOptions`). REA checked the
    /// brightness (88%) in the shipped WinDirStat.exe; see docs/research.md.
    fn default() -> Self {
        Options {
            style: Style::KDirStat,
            grid: false,
            grid_color: [0, 0, 0],
            brightness: 0.88,
            height: 0.38,
            scale_factor: 0.91,
            ambient_light: 0.13,
            light_x: -1.0,
            light_y: -1.0,
        }
    }
}

impl Options {
    fn is_cushion(&self) -> bool {
        self.ambient_light < 1.0 && self.height > 0.0 && self.scale_factor > 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Rect { left, top, right, bottom }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

type Surface = [f64; 4];

struct Leaf {
    rect: Rect,
    surface: Surface,
    color: Rgb,
    /// Brightness multiplier: darker for free space, lighter for unknown
    /// (WinDirStat's COLORFLAG_DARKER / COLORFLAG_LIGHTER).
    gain: f64,
    ext: u32,
}

/// Rectangles from the last render, for hit testing and highlighting.
#[derive(Default)]
pub struct Layout {
    rects: Vec<Rect>,
    visited: Vec<NodeId>,
    root: NodeId,
    width: u32,
    height: u32,
}

impl Layout {
    fn reset(&mut self, node_count: usize, root: NodeId, width: u32, height: u32) {
        if self.rects.len() != node_count {
            self.rects = vec![Rect::default(); node_count];
        } else {
            for &v in &self.visited {
                self.rects[v as usize] = Rect::default();
            }
        }
        self.visited.clear();
        self.root = root;
        self.width = width;
        self.height = height;
    }

    fn set(&mut self, node: NodeId, rect: Rect) {
        self.rects[node as usize] = rect;
        self.visited.push(node);
    }

    /// Where `node` was drawn, if it was large enough to be drawn.
    pub fn rect(&self, node: NodeId) -> Option<Rect> {
        self.rects
            .get(node as usize)
            .copied()
            .filter(|r| r.width() > 0 && r.height() > 0)
    }

    /// The deepest drawn node under a pixel.
    pub fn hit(&self, tree: &Tree, x: i32, y: i32) -> Option<NodeId> {
        let mut node = self.root;
        if !self.rect(node)?.contains(x, y) {
            return None;
        }
        'descend: loop {
            if !tree.node(node).is_dir() {
                return Some(node);
            }
            for child in tree.children(node) {
                if self.rect(child).is_some_and(|r| r.contains(x, y)) {
                    node = child;
                    continue 'descend;
                }
            }
            return Some(node);
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Render the subtree under `root` into an RGBA image of `width` x `height`.
/// `ext_colors` maps extension ids to palette colours already equalised to
/// `PALETTE_BRIGHTNESS`. Files with extension `highlight` get a frame, as
/// when a file type is selected in WinDirStat's extension list.
#[allow(clippy::too_many_arguments)]
pub fn render(
    tree: &Tree,
    root: NodeId,
    width: u32,
    height: u32,
    mode: SizeMode,
    options: &Options,
    ext_colors: &[Rgb],
    highlight: Option<u32>,
    layout: &mut Layout,
) -> Vec<u8> {
    layout.reset(tree.nodes.len(), root, width, height);
    let (w, h) = (width as usize, height as usize);
    let mut pixels = vec![0u8; w * h * 4];
    if w == 0 || h == 0 {
        return pixels;
    }

    let grid = solid(options.grid_color, PALETTE_BRIGHTNESS);
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[grid[0], grid[1], grid[2], 255]);
    }
    if tree.node(root).size(mode) == 0 {
        return pixels;
    }

    let leaves = lay_out(tree, root, Rect::new(0, 0, width as i32, height as i32), mode, options, ext_colors, layout);
    shade(&mut pixels, w, h, &leaves, options);
    if let Some(ext) = highlight.filter(|&e| e != NO_EXT) {
        for leaf in leaves.iter().filter(|l| l.ext == ext) {
            frame(&mut pixels, w, h, leaf.rect);
        }
    }
    pixels
}

/// A white frame with a dark inner line so it reads on any colour.
fn frame(pixels: &mut [u8], w: usize, h: usize, r: Rect) {
    let mut put = |x: i32, y: i32, c: [u8; 3]| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = (y as usize * w + x as usize) * 4;
            pixels[i..i + 3].copy_from_slice(&c);
        }
    };
    let mut ring = |inset: i32, c: [u8; 3]| {
        let (l, t, rr, b) = (r.left + inset, r.top + inset, r.right - 1 - inset, r.bottom - 1 - inset);
        if l > rr || t > b {
            return;
        }
        for x in l..=rr {
            put(x, t, c);
            put(x, b, c);
        }
        for y in t..=b {
            put(l, y, c);
            put(rr, y, c);
        }
    };
    ring(0, [255, 255, 255]);
    ring(1, [0, 0, 0]);
}

fn lay_out(
    tree: &Tree,
    root: NodeId,
    area: Rect,
    mode: SizeMode,
    options: &Options,
    ext_colors: &[Rgb],
    layout: &mut Layout,
) -> Vec<Leaf> {
    struct State {
        node: NodeId,
        rect: Rect,
        surface: Surface,
        h: f64,
        as_root: bool,
    }

    let cushion = options.is_cushion();
    let grid_width = options.grid as i32;
    let fallback = ext_colors.last().copied().unwrap_or([153, 153, 153]);
    let mut leaves = Vec::new();
    let mut stack = vec![State {
        node: root,
        rect: area,
        surface: [0.0; 4],
        h: options.height,
        as_root: true,
    }];
    let mut sizes: Vec<f64> = Vec::new();

    while let Some(mut state) = stack.pop() {
        layout.set(state.node, state.rect);
        let rc = state.rect;
        if rc.width() <= grid_width || rc.height() <= grid_width {
            continue;
        }
        if cushion && !state.as_root {
            add_ridge(&rc, &mut state.surface, state.h);
        }

        let node = tree.node(state.node);
        let children = if node.is_dir() {
            tree.children_by_size(state.node, mode)
        } else {
            Vec::new()
        };
        if children.is_empty() {
            let mut leaf = rc;
            if options.grid {
                leaf.top += 1;
                leaf.left += 1;
                if leaf.width() <= 0 || leaf.height() <= 0 {
                    continue;
                }
            }
            let (color, gain) = match node.kind {
                NodeKind::FreeSpace => (make_bright([100, 100, 100], PALETTE_BRIGHTNESS), 0.66),
                NodeKind::Unknown => (make_bright([255, 255, 0], PALETTE_BRIGHTNESS), 1.2),
                _ => (ext_colors.get(node.ext as usize).copied().unwrap_or(fallback), 1.0),
            };
            leaves.push(Leaf { rect: leaf, surface: state.surface, color, gain, ext: node.ext });
            continue;
        }

        sizes.clear();
        sizes.extend(children.iter().map(|&c| tree.node(c).size(mode) as f64));
        let child_h = state.h * options.scale_factor;
        let mut push = |i: usize, rect: Rect| {
            stack.push(State {
                node: children[i],
                rect,
                surface: state.surface,
                h: child_h,
                as_root: false,
            })
        };
        match options.style {
            Style::KDirStat => kdirstat(&rc, node.size(mode) as f64, &sizes, &mut push),
            Style::Squarified => squarify(&rc, node.size(mode) as f64, &sizes, &mut push),
        }
    }
    leaves
}

fn add_ridge(rc: &Rect, surface: &mut Surface, h: f64) {
    let h4 = 4.0 * h;
    let wf = h4 / rc.width() as f64;
    surface[2] += wf * (rc.right + rc.left) as f64;
    surface[0] -= wf;
    let hf = h4 / rc.height() as f64;
    surface[3] += hf * (rc.bottom + rc.top) as f64;
    surface[1] -= hf;
}

/// KDirStat rows: fill a row with children until the next one would get an
/// aspect ratio below 0.4, then start the next row.
fn kdirstat(rc: &Rect, total: f64, sizes: &[f64], push: &mut impl FnMut(usize, Rect)) {
    const MIN_PROPORTION: f64 = 0.4;
    let horizontal_rows = rc.width() >= rc.height();
    let width = if horizontal_rows {
        if rc.height() > 0 { rc.width() as f64 / rc.height() as f64 } else { 1.0 }
    } else if rc.width() > 0 {
        rc.height() as f64 / rc.width() as f64
    } else {
        1.0
    };

    // Split children into rows; each row has a height (fraction of the rect)
    // and each child a width (fraction of its row).
    let mut rows: Vec<(f64, usize)> = Vec::new();
    let mut child_width = vec![0.0; sizes.len()];
    let mut next = 0;
    while next < sizes.len() {
        let mut used = 0.0;
        let mut row_height = 0.0;
        let mut i = next;
        while i < sizes.len() {
            used += sizes[i];
            let virtual_row_height = used / total;
            let cw = sizes[i] / total * width / virtual_row_height;
            if cw / virtual_row_height < MIN_PROPORTION && i > next {
                break;
            }
            row_height = virtual_row_height;
            i += 1;
        }
        let count = i - next;
        let row_size = total * row_height;
        for j in next..i {
            child_width[j] = sizes[j] / row_size;
        }
        rows.push((row_height, count));
        next = i;
    }

    let (span, depth) = if horizontal_rows {
        (rc.width(), rc.height())
    } else {
        (rc.height(), rc.width())
    };
    let mut top = if horizontal_rows { rc.top } else { rc.left } as f64;
    let mut c = 0;
    for (row, &(row_height, count)) in rows.iter().enumerate() {
        let f_bottom = top + row_height * depth as f64;
        let bottom = if row == rows.len() - 1 {
            if horizontal_rows { rc.bottom } else { rc.right }
        } else {
            f_bottom as i32
        };
        let mut left = if horizontal_rows { rc.left } else { rc.top } as f64;
        for i in 0..count {
            let f_right = left + child_width[c] * span as f64;
            let right = if i == count - 1 {
                if horizontal_rows { rc.right } else { rc.bottom }
            } else {
                f_right as i32
            };
            let child = if horizontal_rows {
                Rect::new(left as i32, top as i32, right, bottom)
            } else {
                Rect::new(top as i32, left as i32, bottom, right)
            };
            push(c, child);
            left = f_right;
            c += 1;
        }
        top = f_bottom;
    }
}

/// SequoiaView-style squarification: grow a row along the shorter side while
/// the worst aspect ratio in it keeps improving.
fn squarify(rc: &Rect, total: f64, sizes: &[f64], push: &mut impl FnMut(usize, Rect)) {
    let mut remaining = *rc;
    let mut remaining_size = total;
    let size_per_square_pixel = total / rc.width() as f64 / rc.height() as f64;
    let mut head = 0;

    while head < sizes.len() {
        let horizontal = remaining.width() >= remaining.height();
        let side = if horizontal { remaining.height() } else { remaining.width() } as f64;
        let hh = side * side * size_per_square_pixel;

        let row_begin = head;
        let mut row_end = head;
        let mut worst = f64::MAX;
        let rmax = sizes[row_begin];
        let mut sum = 0.0;
        while row_end < sizes.len() {
            let rmin = sizes[row_end];
            let ss = (sum + rmin) * (sum + rmin);
            let next_worst = (hh * rmax / ss).max(ss / hh / rmin);
            if next_worst > worst {
                break;
            }
            sum += rmin;
            row_end += 1;
            worst = next_worst;
        }

        let mut width = if horizontal { remaining.width() } else { remaining.height() };
        if sum < remaining_size {
            width = (sum / remaining_size * width as f64) as i32;
        }
        let mut row = remaining;
        if horizontal {
            row.right = row.left + width;
        } else {
            row.bottom = row.top + width;
        }

        let mut f_begin = if horizontal { row.top } else { row.left } as f64;
        let row_len = if horizontal { row.height() } else { row.width() } as f64;
        for i in row_begin..row_end {
            let f_end = f_begin + sizes[i] / sum * row_len;
            let end = if i == row_end - 1 {
                if horizontal { row.bottom } else { row.right }
            } else {
                f_end as i32
            };
            let child = if horizontal {
                Rect::new(row.left, f_begin as i32, row.right, end)
            } else {
                Rect::new(f_begin as i32, row.top, end, row.bottom)
            };
            push(i, child);
            f_begin = f_end;
        }

        if horizontal {
            remaining.left += width;
        } else {
            remaining.top += width;
        }
        remaining_size -= sum;
        head = row_end;
        if remaining.width() <= 0 || remaining.height() <= 0 {
            break;
        }
    }
}

fn solid(color: Rgb, brightness: f64) -> Rgb {
    let f = brightness / PALETTE_BRIGHTNESS;
    normalize(
        (color[0] as f64 * f) as i32,
        (color[1] as f64 * f) as i32,
        (color[2] as f64 * f) as i32,
    )
}

const BAND_ROWS: usize = 32;

fn shade(pixels: &mut [u8], w: usize, h: usize, leaves: &[Leaf], options: &Options) {
    // Bucket leaves by the horizontal bands they touch so each band can be
    // shaded independently with exclusive access to its rows.
    let bands = h.div_ceil(BAND_ROWS);
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for (i, leaf) in leaves.iter().enumerate() {
        let top = leaf.rect.top.max(0) as usize;
        let bottom = (leaf.rect.bottom.max(0) as usize).min(h);
        if top >= bottom {
            continue;
        }
        for band in top / BAND_ROWS..=(bottom - 1) / BAND_ROWS {
            buckets[band].push(i as u32);
        }
    }

    let len = (options.light_x.powi(2) + options.light_y.powi(2) + 100.0).sqrt();
    let light = (options.light_x / len, options.light_y / len, 10.0 / len);
    let cushion = options.is_cushion();
    let ia = options.ambient_light;
    let is = 1.0 - ia;

    pixels
        .par_chunks_mut(w * 4 * BAND_ROWS)
        .zip(buckets.par_iter())
        .enumerate()
        .for_each(|(band, (rows, bucket))| {
            let band_top = band * BAND_ROWS;
            let band_bottom = (band_top + BAND_ROWS).min(h);
            for &i in bucket {
                let leaf = &leaves[i as usize];
                let r = leaf.rect;
                let y0 = (r.top.max(0) as usize).max(band_top);
                let y1 = (r.bottom.max(0) as usize).min(band_bottom);
                let x0 = r.left.max(0) as usize;
                let x1 = (r.right.max(0) as usize).min(w);
                let [cr, cg, cb] = leaf.color.map(|c| c as f64);
                let s = leaf.surface;
                // Lighter items are capped at full brightness (WinDirStat).
                let brightness = options.brightness * leaf.gain;
                let brightness = if leaf.gain > 1.0 { brightness.min(1.0) } else { brightness };
                let gain = brightness / PALETTE_BRIGHTNESS;
                let flat = (!cushion).then(|| solid(leaf.color, brightness));
                for y in y0..y1 {
                    let row = &mut rows[(y - band_top) * w * 4..][..w * 4];
                    let ny = -(2.0 * s[1] * (y as f64 + 0.5) + s[3]);
                    for x in x0..x1 {
                        let rgb = match flat {
                            Some(c) => c,
                            None => {
                                let nx = -(2.0 * s[0] * (x as f64 + 0.5) + s[2]);
                                let cosa = ((nx * light.0 + ny * light.1 + light.2)
                                    / (nx * nx + ny * ny + 1.0).sqrt())
                                .min(1.0);
                                let pixel = ((is * cosa).max(0.0) + ia) * gain;
                                normalize(
                                    (cr * pixel) as i32,
                                    (cg * pixel) as i32,
                                    (cb * pixel) as i32,
                                )
                            }
                        };
                        row[x * 4..x * 4 + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NO_EXT, NO_NODE, Node, NodeKind};

    fn node(name: &str, size: u64, parent: NodeId, kind: NodeKind) -> Node {
        Node {
            name: name.into(),
            logical: size,
            physical: size,
            modified: 0,
            parent,
            first_child: NO_NODE,
            child_count: 0,
            files: 1,
            dirs: 0,
            attributes: 0,
            ext: if kind == NodeKind::File { 0 } else { NO_EXT },
            kind,
            flags: 0,
        }
    }

    /// root (dir) -> [a (dir, 60) -> [x 40, y 20], b 30, c 10]
    fn sample() -> Tree {
        let mut nodes = vec![
            node("root", 100, NO_NODE, NodeKind::Dir),
            node("a", 60, 0, NodeKind::Dir),
            node("b", 30, 0, NodeKind::File),
            node("c", 10, 0, NodeKind::File),
            node("x", 40, 1, NodeKind::File),
            node("y", 20, 1, NodeKind::File),
        ];
        nodes[0].first_child = 1;
        nodes[0].child_count = 3;
        nodes[1].first_child = 4;
        nodes[1].child_count = 2;
        Tree { nodes, extensions: vec![".bin".into()], errors: 0 }
    }

    fn leaf_area(style: Style) {
        let tree = sample();
        let mut layout = Layout::default();
        let options = Options { style, ..Options::default() };
        let px = render(&tree, 0, 200, 100, SizeMode::Physical, &options, &[[0, 0, 255]], None, &mut layout);
        assert_eq!(px.len(), 200 * 100 * 4);
        let files = [2u32, 3, 4, 5];
        let area: i32 = files
            .iter()
            .map(|&f| layout.rect(f).map_or(0, |r| r.width() * r.height()))
            .sum();
        assert_eq!(area, 200 * 100, "leaves partition the area");
        for (i, &a) in files.iter().enumerate() {
            for &b in &files[i + 1..] {
                let (ra, rb) = (layout.rect(a).unwrap(), layout.rect(b).unwrap());
                let overlap = ra.left < rb.right && rb.left < ra.right && ra.top < rb.bottom && rb.top < ra.bottom;
                assert!(!overlap, "{ra:?} overlaps {rb:?}");
            }
        }
        // Areas follow sizes within rounding.
        let x = layout.rect(4).unwrap();
        assert!(((x.width() * x.height()) as f64 / 20000.0 - 0.4).abs() < 0.03);
    }

    #[test]
    fn kdirstat_partitions_area() {
        leaf_area(Style::KDirStat);
    }

    #[test]
    fn squarified_partitions_area() {
        leaf_area(Style::Squarified);
    }

    #[test]
    fn hit_test_finds_deepest_node() {
        let tree = sample();
        let mut layout = Layout::default();
        render(&tree, 0, 200, 100, SizeMode::Physical, &Options::default(), &[[0, 0, 255]], None, &mut layout);
        for f in [2u32, 3, 4, 5] {
            let r = layout.rect(f).unwrap();
            assert_eq!(layout.hit(&tree, r.left, r.top), Some(f));
        }
        assert_eq!(layout.hit(&tree, 500, 500), None);
    }

    #[test]
    fn cushion_is_brightest_toward_the_light() {
        let tree = sample();
        let mut layout = Layout::default();
        let px = render(&tree, 0, 200, 100, SizeMode::Physical, &Options::default(), &[[102, 102, 255]], None, &mut layout);
        let r = layout.rect(4).unwrap();
        let lum = |x: i32, y: i32| {
            let i = ((y as usize) * 200 + x as usize) * 4;
            px[i] as u32 + px[i + 1] as u32 + px[i + 2] as u32
        };
        // Light comes from the top left, so that corner beats the bottom right.
        assert!(lum(r.left + 2, r.top + 2) > lum(r.right - 3, r.bottom - 3));
    }
}
