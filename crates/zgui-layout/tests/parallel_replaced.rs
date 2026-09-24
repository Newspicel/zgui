//! A pooled layout sizes an inline replaced box the way a serial one does.
//!
//! The fixture is a column of flex cells, each holding a block that holds one `inline-block`
//! replaced box of a CSS size. That is an icon inside a table cell. The measurer forks, so the
//! pooled pass lays the cells out on workers with their own measurers.

mod support;

use support::mono::MonoShaper;
use support::{Element, Fixture};
use zgui_layout::FormattingContext;
use zgui_layout::NaturalSize;
use zgui_layout::measure::{
    MeasureContent, MeasureRequest, Measured, ShapedSummary, WorkerMeasure,
};
use zgui_layout::style::DeviceStyle;
use zgui_layout::text::Paragraphs;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::parallel::LayoutPool;
use zgui_layout::tree::store::LayoutStore;
use zgui_text::{BreakRequest, BrokenParagraph, ClusterRun, ParagraphContent, ParagraphKey};

/// How many cells the fixture holds.
const CELLS: usize = 320;

/// The measurer a window runs: a shaper that forks, and natural sizes for replaced content.
struct Forking(Paragraphs<MonoShaper, NaturalSize>);

impl zgui_text::ShapedClusters for Forking {
    fn visit_clusters(
        &self,
        paragraph: ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(ClusterRun<'_>),
    ) {
        self.0.visit_clusters(paragraph, line, visit);
    }
}

impl MeasureContent for Forking {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        self.0.measure(request)
    }

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedSummary {
        self.0.shape(content)
    }

    fn break_lines(&mut self, key: ParagraphKey, request: &BreakRequest<'_>) -> BrokenParagraph {
        self.0.break_lines(key, request)
    }

    fn strut(&mut self, style: &zgui_text_style::TextStyle) -> zgui_text::StrutMetrics {
        self.0.strut(style)
    }

    fn paint_slot(&mut self, paint: &zgui_text_style::TextPaint) -> zgui_text::Brush {
        self.0.paint_slot(paint)
    }

    fn fork_measurer(&mut self, owned: &[ParagraphKey]) -> Option<Box<dyn WorkerMeasure>> {
        self.0
            .fork_worker(owned)
            .map(|worker| Box::new(worker) as Box<dyn WorkerMeasure>)
    }

    fn absorb_measurer(&mut self, worker: Box<dyn WorkerMeasure>) {
        let worker = worker
            .into_any()
            .downcast::<Paragraphs<MonoShaper, NaturalSize>>()
            .expect("a fork comes back to the measurer that made it");
        self.0.absorb_worker(*worker);
    }
}

/// The column of cells.
fn fixture() -> Fixture {
    let cells = (0..CELLS)
        .map(|_| {
            Element::new("cell").children(vec![
                Element::new("mark").children(vec![Element::new("picture").image(24.0, 24.0)]),
            ])
        })
        .collect();
    Fixture::with_natural_size(
        Element::new("root").children(cells),
        "root { display: flex; flex-direction: column; width: 400px }
         cell { display: flex; align-items: center; justify-content: center; min-height: 26px }
         mark { display: block }
         picture { display: inline-block; width: 14px; height: 14px }",
        (24.0, 24.0),
    )
}

/// The size of every replaced box, in key order.
fn pictures(pool: Option<&LayoutPool>) -> Vec<(f32, f32)> {
    let fixture = fixture();
    let mut store: LayoutStore = fixture.box_tree();
    let mut content = Forking(Paragraphs::with_replaced(
        MonoShaper::default(),
        NaturalSize,
    ));
    if let Some(pool) = pool {
        // The window's order: the dirty inline contexts are flattened and shaped first.
        let jobs = {
            let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
            zgui_layout::text::preshape::collect(&mut tree)
        };
        content.0.pre_shape(&jobs.contents(), pool);
    }
    {
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        if let Some(pool) = pool {
            tree = tree.with_parallel(pool);
        }
        let before = zgui_profile::counter::snapshot().layout_batches_distributed;
        assert!(tree.layout_viewport(800.0, 600.0));
        if pool.is_some() {
            assert!(
                zgui_profile::counter::snapshot().layout_batches_distributed > before,
                "the pooled pass distributed no batch"
            );
        }
    }
    let mut keys = store.keys();
    keys.sort_unstable();
    keys.into_iter()
        .filter(|&key| store.node(key).fc == FormattingContext::Replaced)
        .map(|key| {
            let layout = store.layout_of(key).expect("laid out");
            (layout.size.width.0, layout.size.height.0)
        })
        .collect()
}

#[test]
fn a_pooled_layout_sizes_inline_replaced_boxes_as_a_serial_one_does() {
    let serial = pictures(None);
    assert_eq!(serial.len(), CELLS);
    assert!(
        serial.iter().all(|&size| size == (14.0, 14.0)),
        "{serial:?}"
    );
    let pool = LayoutPool::new(4);
    let pooled = pictures(Some(&pool));
    assert_eq!(pooled, serial);
}
