//! Packing a frame's passes into scratch layers, and the layers into rounds.
//!
//! A pass is rasterised into a bin of a scratch layer and composited from there. Each bin is a
//! rectangle of the pass's own size, placed on a shelf of its layer, so two passes never share a
//! texel whatever they cover on the surface. What a layer holds is therefore set by area, and a
//! stack of passes over one point costs no more than the same passes side by side.
//!
//! A frame can still have more passes than one scratch holds. The passes are then cut into
//! **rounds**: a run of passes, in plan order, that fits in the scratch at once. A renderer
//! rasterises one round, composites its passes, and rasterises the next round into the same
//! layers. Paint order does not change, because a round is a contiguous run of the plan and every
//! composite still reads its own pass. No pass is ever left without a place.
//!
//! # What packing rests on
//!
//! **A pass paints inside its region and nowhere else.** A region is the union of its items' inks,
//! each widened by the whole reach of its stroke and rounded outwards onto a sixteen-pixel grid. A
//! rasteriser moves a pass into its bin and clips a cut pass to it, so a pass that broke this would
//! land in the bin beside it.
//!
//! Neither implementation is obliged to use this, which is why it is a helper and not part of the
//! [`VectorRaster`](crate::VectorRaster) contract.

use zgui_geom::{Device, Point, Rect, Size};

use crate::vector::target::VectorTarget;

/// How much scratch one round may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    /// The side of one layer, in device pixels.
    ///
    /// A layer grows past it to hold a pass wider or taller than this, so every pass fits.
    pub side: i32,
    /// The most layers one round may use.
    pub layers: u32,
}

impl Capacity {
    /// What a rasteriser holds unless it is told otherwise: eight layers of 2048 by 2048.
    ///
    /// That is 128 MiB of straight colour at most, and room for about 33 million pixels of passes
    /// in one round. A frame that needs more takes another round, which costs one more submission
    /// and no more rasterisation.
    pub const DEFAULT: Self = Self {
        side: 2048,
        layers: 8,
    };
}

impl Default for Capacity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Where one pass goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The round that rasterises it.
    pub round: usize,
    /// The layer it goes into.
    pub target: VectorTarget,
    /// Its bin in that layer.
    pub bin: Rect<i32, Device>,
}

/// A frame's passes, each with a round, a layer and a bin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layering {
    /// One placement per pass, in the plan's order.
    placements: Vec<Placement>,
    /// The texels across every layer has to have.
    width: u32,
    /// The texels down every layer has to have.
    height: u32,
    /// The most layers any round uses.
    layers: u32,
    /// How many rounds the passes take.
    rounds: usize,
}

impl Layering {
    /// Packs `regions`, in order, into rounds no larger than `capacity`.
    ///
    /// Shelf packing: a bin goes right of the last one, on a new shelf below when the shelf is
    /// full, on a new layer when the layer is full, and into a new round when the round is full.
    /// The shelf width follows the frame's total area, so a frame of a few passes uses a small,
    /// square part of one layer.
    pub fn of(regions: &[Rect<i32, Device>], capacity: Capacity) -> Self {
        let mut shelves = Shelves::new(regions, capacity);
        let placements = regions
            .iter()
            .map(|region| shelves.place(region.size.non_negative()))
            .collect();
        Self {
            placements,
            width: shelves.width as u32,
            height: shelves.height as u32,
            layers: shelves.layers,
            rounds: if regions.is_empty() {
                0
            } else {
                shelves.round + 1
            },
        }
    }

    /// Where pass `index` goes, or `None` for a pass that was not packed.
    pub fn placement(&self, index: usize) -> Option<Placement> {
        self.placements.get(index).copied()
    }

    /// The layer pass `index` goes into, or [`VectorTarget::NONE`] for one that was not packed.
    pub fn target(&self, index: usize) -> VectorTarget {
        self.placement(index)
            .map_or(VectorTarget::NONE, |placement| placement.target)
    }

    /// The most layers any round uses.
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// How many rounds the passes take.
    pub fn rounds(&self) -> usize {
        self.rounds
    }

    /// The width and height every layer has to have to hold its bins.
    pub fn extent(&self) -> (u32, u32) {
        (self.width.max(1), self.height.max(1))
    }
}

/// The packing cursor.
struct Shelves {
    /// The widest a shelf may be.
    shelf: i32,
    /// The tallest a layer may be.
    page: i32,
    /// The most layers a round may use.
    ceiling: u32,
    /// The round being filled.
    round: usize,
    /// The layer being filled.
    layer: u32,
    /// Where the next bin starts on the open shelf.
    x: i32,
    /// The top of the open shelf.
    y: i32,
    /// The height of the open shelf.
    row: i32,
    /// The widest any layer reaches.
    width: i32,
    /// The tallest any layer reaches.
    height: i32,
    /// The most layers any round opens.
    layers: u32,
}

impl Shelves {
    /// A cursor at the start of the first layer, sized for `regions`.
    fn new(regions: &[Rect<i32, Device>], capacity: Capacity) -> Self {
        let sizes = regions.iter().map(|region| region.size.non_negative());
        let widest = sizes.clone().map(|size| size.width).max().unwrap_or(0);
        let tallest = sizes.clone().map(|size| size.height).max().unwrap_or(0);
        let area: i64 = sizes
            .map(|size| i64::from(size.width) * i64::from(size.height))
            .sum();
        let side = capacity.side.max(1);
        let square = (area as f64).sqrt().ceil() as i32;
        Self {
            shelf: square.clamp(widest.max(1), side.max(widest)),
            page: side.max(tallest),
            ceiling: capacity.layers.max(1),
            round: 0,
            layer: 0,
            x: 0,
            y: 0,
            row: 0,
            width: 0,
            height: 0,
            layers: 0,
        }
    }

    /// Places one bin of `size`.
    fn place(&mut self, size: Size<i32, Device>) -> Placement {
        if size.width == 0 || size.height == 0 {
            return self.at(Point::new(0, 0), Size::new(0, 0));
        }
        if self.x > 0 && self.x + size.width > self.shelf {
            self.y += self.row;
            self.x = 0;
            self.row = 0;
        }
        if self.y > 0 && self.y + size.height > self.page {
            self.next_layer();
        }
        let origin = Point::new(self.x, self.y);
        self.x += size.width;
        self.row = self.row.max(size.height);
        self.width = self.width.max(self.x);
        self.height = self.height.max(self.y + self.row);
        self.layers = self.layers.max(self.layer + 1);
        self.at(origin, size)
    }

    /// Moves to the start of the next layer, or of the next round when this one is full.
    fn next_layer(&mut self) {
        self.layer += 1;
        if self.layer >= self.ceiling {
            self.layer = 0;
            self.round += 1;
        }
        self.x = 0;
        self.y = 0;
        self.row = 0;
    }

    /// A placement in the open layer.
    fn at(&self, origin: Point<i32, Device>, size: Size<i32, Device>) -> Placement {
        Placement {
            round: self.round,
            target: VectorTarget(u64::from(self.layer)),
            bin: Rect::new(origin, size),
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, Point, Rect, Size};

    use super::{Capacity, Layering};
    use crate::vector::target::VectorTarget;

    /// A tile-aligned region.
    fn region(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    /// Every pair of passes in one round and one layer has bins that do not meet.
    fn assert_disjoint(layering: &Layering, count: usize) {
        for one in 0..count {
            for two in one + 1..count {
                let (Some(first), Some(second)) =
                    (layering.placement(one), layering.placement(two))
                else {
                    panic!("pass {one} or {two} was not placed");
                };
                if first.round == second.round && first.target == second.target {
                    assert!(
                        first.bin.intersection(second.bin).is_none(),
                        "passes {one} and {two} share texels: {first:?} and {second:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn two_disjoint_passes_share_a_layer() {
        let regions = [region(0, 0, 32, 32), region(64, 0, 32, 32)];
        let layering = Layering::of(&regions, Capacity::DEFAULT);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.rounds(), 1);
        assert_eq!(layering.target(0), layering.target(1));
        assert_disjoint(&layering, regions.len());
    }

    #[test]
    fn passes_stacked_over_one_point_share_a_layer_in_bins_of_their_own() {
        let regions: Vec<_> = (0..500).map(|_| region(0, 0, 64, 64)).collect();
        let layering = Layering::of(&regions, Capacity::DEFAULT);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.rounds(), 1);
        assert_disjoint(&layering, regions.len());
    }

    #[test]
    fn a_bin_has_the_size_of_its_region() {
        let regions = [region(16, 32, 48, 80), region(0, 0, 16, 16)];
        let layering = Layering::of(&regions, Capacity::DEFAULT);
        for (index, region) in regions.iter().enumerate() {
            let placement = layering.placement(index).expect("placed");
            assert_eq!(placement.bin.size, region.size);
        }
    }

    #[test]
    fn far_away_passes_are_packed_without_the_space_between_them() {
        let regions = [
            region(10_000, 20_000, 16, 16),
            region(30_000, 40_000, 16, 16),
        ];
        let layering = Layering::of(&regions, Capacity::DEFAULT);
        let (width, height) = layering.extent();
        assert!(
            width <= 32 && height <= 32,
            "scratch was {width} by {height}"
        );
    }

    #[test]
    fn a_full_layer_opens_the_next_one() {
        let capacity = Capacity {
            side: 64,
            layers: 4,
        };
        let regions: Vec<_> = (0..8).map(|_| region(0, 0, 64, 32)).collect();
        let layering = Layering::of(&regions, capacity);
        assert_eq!(layering.layers(), 4);
        assert_eq!(layering.rounds(), 1);
        assert_eq!(layering.target(2), VectorTarget(1));
        assert_disjoint(&layering, regions.len());
    }

    #[test]
    fn a_full_round_starts_the_next_one_and_rounds_follow_the_plan_order() {
        let capacity = Capacity {
            side: 32,
            layers: 2,
        };
        let regions: Vec<_> = (0..7).map(|_| region(0, 0, 32, 32)).collect();
        let layering = Layering::of(&regions, capacity);
        assert_eq!(layering.rounds(), 4);
        assert_eq!(layering.layers(), 2);
        let rounds: Vec<_> = (0..regions.len())
            .map(|index| layering.placement(index).expect("placed").round)
            .collect();
        assert_eq!(rounds, [0, 0, 1, 1, 2, 2, 3]);
        assert_disjoint(&layering, regions.len());
    }

    #[test]
    fn a_pass_larger_than_a_layer_widens_the_layer() {
        let capacity = Capacity {
            side: 64,
            layers: 1,
        };
        let regions = [region(0, 0, 640, 480), region(0, 0, 16, 16)];
        let layering = Layering::of(&regions, capacity);
        let (width, height) = layering.extent();
        assert!(width >= 640 && height >= 480);
        assert_eq!(layering.placement(0).expect("placed").round, 0);
        assert_disjoint(&layering, regions.len());
    }

    #[test]
    fn an_empty_region_costs_no_space() {
        let regions = [region(0, 0, 0, 0), region(0, 0, 16, 16)];
        let layering = Layering::of(&regions, Capacity::DEFAULT);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.extent(), (16, 16));
    }

    #[test]
    fn no_passes_take_no_rounds() {
        let layering = Layering::of(&[], Capacity::DEFAULT);
        assert_eq!(layering.rounds(), 0);
        assert_eq!(layering.layers(), 0);
    }
}
