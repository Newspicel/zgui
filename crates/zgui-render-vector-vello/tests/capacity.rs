//! How much scratch a frame costs, and how a frame larger than its scratch is still drawn whole.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Point, Rect, Size};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_render::{Capacity, Renderer, VectorError, VectorFrame, VectorRaster};
use zgui_render_vector_coverage::CoverageRaster;
use zgui_render_vector_vello::VelloRaster;
use zgui_render_vector_vello::raster::scratch::Scratch;
use zgui_render_wgpu::frame::vector::VectorSource;
use zgui_render_wgpu::{Pixels, WgpuRenderer};
use zgui_scene::{ClipId, Scene};

use support::{Harness, Which, harness_at, opaque, path, present, quad, rect, scene_at, vector};

/// The extent every scene here fits in.
const SIDE: i32 = 640;

/// The side of one square in the grid scenes below.
const SQUARE: f32 = 8.0;

/// How far apart the grid scenes put their squares.
const PITCH: f32 = 16.0;

/// How many squares fit across the grid.
const COLUMNS: u32 = 39;

/// A scratch so small that a frame of a few hundred passes takes dozens of rounds.
const SMALL: Capacity = Capacity {
    side: 64,
    layers: 2,
};

/// How many steps the staircase scenes have.
const STEPS: u32 = 160;

/// Where square `index` of a grid scene is drawn.
fn cell(index: u32) -> (f32, f32) {
    (
        PITCH + (index % COLUMNS) as f32 * PITCH,
        PITCH + (index / COLUMNS) as f32 * PITCH,
    )
}

/// The middle of the square `index` draws, which is where the picture is read.
fn middle_of(index: u32) -> (i32, i32) {
    let (x, y) = cell(index);
    (x as i32 + 4, y as i32 + 4)
}

/// A scene of `count` squares that nowhere touch, each with something drawn over it so each needs a
/// pass of its own.
///
/// The quad over each square is what splits the passes: it is an intervening primitive that covers
/// an item already accumulated, so the next square cannot join the same pass however far away it is.
fn one_pass_each(count: u32) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..count {
        let (x, y) = cell(index);
        vector(
            &mut scene,
            index,
            path(rect(x, y, SQUARE, SQUARE)),
            opaque(255, 255, 255),
            ClipId::ROOT,
        );
        quad(
            &mut scene,
            rect(x, y, SQUARE, SQUARE),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    scene.finish(&DamageSet::full());
    scene
}

/// A scene of `count` squares stacked exactly on top of each other, each in a pass of its own.
///
/// Every square but the last is white and the last is red, so the pixel in the middle says whether
/// the last pass was drawn, and drawn last.
fn one_stack(count: u32) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..count {
        let colour = if index + 1 == count {
            opaque(255, 0, 0)
        } else {
            opaque(255, 255, 255)
        };
        vector(
            &mut scene,
            index,
            path(rect(16.0, 16.0, 24.0, 24.0)),
            colour,
            ClipId::ROOT,
        );
        quad(
            &mut scene,
            rect(16.0, 16.0, 24.0, 24.0),
            Color::srgb_u8(0, 0, 0, 0),
        );
    }
    scene.finish(&DamageSet::full());
    scene
}

/// The colour of step `index` of a staircase painted in `shade`.
fn step_colour(index: u32, shade: u8) -> [u8; 3] {
    [index as u8, 255 - index as u8, shade]
}

/// The left edge of step `index`.
fn step_left(index: u32) -> f32 {
    16.0 + 2.0 * index as f32
}

/// A staircase of [`STEPS`] squares, each two pixels right of the one before and each in a pass
/// of its own, finished against `damage`.
///
/// Each square covers the left part of every square before it, so the two-pixel strip at the left
/// edge of step `index` shows that step's colour only when every pass is drawn, and drawn in order.
fn staircase(shade: u8, damage: &DamageSet) -> Scene {
    let mut scene = scene_at(SIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    for index in 0..STEPS {
        let [red, green, blue] = step_colour(index, shade);
        let square = rect(step_left(index), 16.0, 24.0, 24.0);
        vector(
            &mut scene,
            index,
            path(square),
            opaque(red, green, blue),
            ClipId::ROOT,
        );
        quad(&mut scene, square, Color::srgb_u8(0, 0, 0, 0));
    }
    scene.finish(damage);
    scene
}

/// Asserts that every step of a staircase painted in `shade` shows at its left edge.
fn assert_steps_in_order(pixels: &Pixels, shade: u8, label: &str) {
    for index in 0..STEPS {
        let x = step_left(index) as i32;
        let [red, green, blue, alpha] = pixels.rgba(x, 28);
        let want = step_colour(index, shade);
        let close = [red, green, blue]
            .iter()
            .zip(want)
            .all(|(got, want)| got.abs_diff(want) <= 2);
        assert!(
            close && alpha == 255,
            "{label}: step {index} reads {:?} where {want:?} belongs",
            [red, green, blue, alpha]
        );
    }
}

/// A rasteriser of `which` kind on `renderer`'s device, holding `capacity` per round.
fn raster_with(
    renderer: &WgpuRenderer,
    which: Which,
    capacity: Capacity,
) -> Option<Box<dyn VectorSource>> {
    let gpu = renderer.gpu();
    let side = SIDE as u32;
    match which {
        Which::Vello => match VelloRaster::new(gpu, side, side) {
            Ok(raster) => Some(Box::new(raster.with_capacity(capacity))),
            Err(failure) => {
                eprintln!("skipped: this device runs no path renderer ({failure})");
                None
            }
        },
        Which::Coverage => Some(Box::new(
            CoverageRaster::new(gpu, side, side).with_capacity(capacity),
        )),
    }
}

/// What rasterising one frame through a rasteriser of its own came to.
struct Rasterised {
    /// What each round reported.
    outcome: Result<(), VectorError>,
    /// How many layers the fullest round used.
    depth: u32,
    /// How many rounds the frame took.
    rounds: usize,
}

/// Rasterises every round of `scene` through a rasteriser of its own on the harness's device.
///
/// A rasteriser of its own rather than the harness's, because the harness's is behind the renderer
/// and what is wanted here is the packing, which a picture cannot show. The device is the
/// harness's: opening a second one would wait on the lock the harness is holding.
fn rasterise(harness: &Harness, scene: &Scene, capacity: Capacity) -> Rasterised {
    let gpu = std::sync::Arc::clone(harness.gpu());
    let mut raster = VelloRaster::new(&gpu, SIDE as u32, SIDE as u32)
        .expect("a rasteriser")
        .with_capacity(capacity);
    let plan = raster.plan(scene.pass_plan());
    let placements = zgui_scene::Placements::of(&scene.spatial);
    let mut outcome = Ok(());
    for round in 0..plan.rounds() {
        raster.clear_targets(&plan, round);
        let mut frame = VectorFrame::new(
            &plan,
            &scene.primitives.vectors,
            &scene.clips,
            &scene.paints,
            &placements,
        )
        .in_round(round);
        outcome = outcome.and(raster.prepare(&mut frame));
    }
    Rasterised {
        outcome,
        depth: raster.depth(),
        rounds: raster.rounds(),
    }
}

/// Two passes that do not meet on the surface go into one layer, and each still composites where it
/// belongs.
#[test]
fn two_disjoint_passes_share_a_layer_and_composite_in_order() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let scene = one_pass_each(2);
    assert_eq!(
        scene.pass_plan().len(),
        2,
        "the fixture has to plan two passes, or nothing here is exercised"
    );
    let pixels = present(&mut harness.renderer, &scene);
    for index in 0..2 {
        let (x, y) = middle_of(index);
        assert_eq!(
            pixels.rgba(x, y),
            [255, 255, 255, 255],
            "square {index} was not composited"
        );
    }
    let rasterised = rasterise(&harness, &scene, Capacity::DEFAULT);
    assert_eq!(
        rasterised.depth, 1,
        "two passes that cannot touch each other must not cost two layers"
    );
}

/// Two passes over the same point share a layer, each in a bin of its own, and the upper one stays
/// on top.
///
/// A bin is a pass's own rectangle of the layer, so what the passes cover on the surface does not
/// decide where they go. What has to hold is that the first composite still reads the first pass's
/// paths and not the second's.
#[test]
fn two_overlapping_passes_share_a_layer_in_bins_of_their_own() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let scene = one_stack(2);
    assert_eq!(scene.pass_plan().len(), 2, "the fixture has to plan two");
    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(24, 24),
        [255, 0, 0, 255],
        "the upper of two stacked passes has to be the one on top"
    );
    let rasterised = rasterise(&harness, &scene, Capacity::DEFAULT);
    assert_eq!(rasterised.depth, 1, "two small passes fit in one layer");
}

/// A frame of five hundred passes draws all five hundred of them, in one layer.
///
/// Five hundred is the shape of a real document — a page of icons, borders and rules, each with
/// something painted over it.
#[test]
fn a_frame_planning_five_hundred_passes_draws_all_of_them() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let count = 500;
    let scene = one_pass_each(count);
    assert_eq!(
        scene.pass_plan().len() as u32,
        count,
        "the fixture has to plan five hundred passes, or nothing here is exercised"
    );
    let dropped_before = counter::get(Counter::VectorFramesDropped);
    let pixels = present(&mut harness.renderer, &scene);
    for index in 0..count {
        let (x, y) = middle_of(index);
        assert_eq!(
            pixels.rgba(x, y),
            [255, 255, 255, 255],
            "square {index} of {count} is missing"
        );
    }
    let rasterised = rasterise(&harness, &scene, Capacity::DEFAULT);
    rasterised
        .outcome
        .expect("five hundred passes that nowhere touch each other all fit");
    assert_eq!(
        rasterised.depth, 1,
        "five hundred small passes cost one layer between them"
    );
    if COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::VectorFramesDropped),
            dropped_before,
            "nothing was dropped, so nothing may be counted as dropped"
        );
    }
}

/// A stack of passes four times deeper than a round's most layers draws every pass, in one layer.
///
/// This is the frame that used to run out: a layer per pass over one point, and a ceiling of
/// sixty-four. A map tile with its labels between its roads, rivers and borders is such a stack.
#[test]
fn a_stack_deeper_than_the_most_layers_draws_every_pass() {
    let Some(mut harness) = harness_at(SIDE, Which::Vello) else {
        return;
    };
    let deep = Scratch::MAX_LAYERS * 4;
    let scene = one_stack(deep);
    assert_eq!(
        scene.pass_plan().len() as u32,
        deep,
        "the fixture has to plan more stacked passes than there are layers"
    );
    let dropped_before = counter::get(Counter::VectorFramesDropped);
    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(
        pixels.rgba(24, 24),
        [255, 0, 0, 255],
        "the last of {deep} stacked passes has to be drawn, and on top"
    );
    let rasterised = rasterise(&harness, &scene, Capacity::DEFAULT);
    rasterised.outcome.expect("every pass has a bin");
    assert_eq!(
        (rasterised.depth, rasterised.rounds),
        (1, 1),
        "small passes cost area, not a layer each"
    );
    if COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::VectorFramesDropped),
            dropped_before,
            "nothing was dropped, so nothing may be counted as dropped"
        );
    }
}

/// A frame with far more passes than its scratch holds draws every one of them, in paint order.
///
/// The scratch is shrunk until the staircase takes dozens of rounds. Each round is rasterised
/// after the composites of the round before it, into the same layers, so the picture has to be
/// the one a scratch large enough for one round draws.
#[test]
fn a_frame_larger_than_its_scratch_draws_every_pass_in_order() {
    for which in [Which::Vello, Which::Coverage] {
        let Some(mut harness) = harness_at(SIDE, which) else {
            return;
        };
        let scene = staircase(64, &DamageSet::full());
        assert_eq!(
            scene.pass_plan().len() as u32,
            STEPS,
            "the fixture has to plan one pass per step"
        );
        let one_round = present(&mut harness.renderer, &scene);
        assert_steps_in_order(&one_round, 64, &format!("{which:?} in one round"));

        if which == Which::Vello {
            let rasterised = rasterise(&harness, &scene, SMALL);
            rasterised.outcome.expect("every round fits");
            assert!(
                rasterised.rounds > 20,
                "the small scratch has to take many rounds, not {}",
                rasterised.rounds
            );
            assert!(rasterised.depth <= SMALL.layers);
        }

        let Some(small) = raster_with(&harness.renderer, which, SMALL) else {
            return;
        };
        harness.renderer.set_vector_raster(small);
        let dropped_before = counter::get(Counter::VectorFramesDropped);
        let rounds = present(&mut harness.renderer, &scene);
        assert_steps_in_order(&rounds, 64, &format!("{which:?} in rounds"));
        assert_eq!(
            rounds.max_difference(&one_round),
            0,
            "{which:?}: rounds changed the picture"
        );
        if COUNTERS_ENABLED {
            assert_eq!(
                counter::get(Counter::VectorFramesDropped),
                dropped_before,
                "{which:?}: a frame drawn in rounds drops nothing"
            );
        }
    }
}

/// A frame of several damage rectangles and several rounds draws each rectangle with every round.
///
/// Each rectangle replays the frame's batches, so the rounds run again from the first one for the
/// second rectangle. Both renderers draw the same two frames; one has a scratch so small that the
/// second frame takes many rounds, and the other draws it in one.
#[test]
fn rounds_run_again_for_each_damaged_rectangle() {
    for which in [Which::Vello, Which::Coverage] {
        let Some((mut small, mut large)) = support::twins(SIDE, which) else {
            return;
        };
        let Some(raster) = raster_with(&small.renderer, which, SMALL) else {
            return;
        };
        small.renderer.set_vector_raster(raster);

        let first = staircase(64, &DamageSet::full());
        present(&mut small.renderer, &first);
        present(&mut large, &first);

        let mut damage = DamageSet::new();
        damage.absorb(Rect::new(Point::new(0, 0), Size::new(160, SIDE)));
        damage.absorb(Rect::new(Point::new(224, 0), Size::new(160, SIDE)));
        assert_eq!(damage.rects().len(), 2, "the fixture needs two rectangles");
        let second = staircase(192, &damage);
        let drawn = |renderer: &mut WgpuRenderer| {
            let outcome = renderer.draw(&second, &damage);
            assert!(outcome.stats().is_some(), "{which:?}: {outcome:?}");
            renderer
                .read_presented()
                .expect("a stand-in surface can be read back")
        };
        let in_rounds = drawn(&mut small.renderer);
        let in_one = drawn(&mut large);
        assert_eq!(
            support::difference(SIDE, &in_rounds, &in_one),
            None,
            "{which:?}: a damaged rectangle drawn in rounds differs from one drawn in one"
        );
        let [red, green, blue] = step_colour(8, 192);
        let x = step_left(8) as i32;
        let [r, g, b, _] = in_rounds.rgba(x, 28);
        assert!(
            r.abs_diff(red) <= 2 && g.abs_diff(green) <= 2 && b.abs_diff(blue) <= 2,
            "{which:?}: the first rectangle has to show the second frame"
        );
    }
}
