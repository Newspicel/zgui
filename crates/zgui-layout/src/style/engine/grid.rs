//! Grid container and item properties, built once per lowering.
//!
//! The other engine walked the computed style's track lists through lazy iterators on every read.
//! The incremental engine takes one owned [`GridContainerStyle`] behind an `Arc`, so a lowering
//! builds it eagerly and every box holding the lowering shares it. A style whose grid properties
//! are all initial carries none.

use std::sync::Arc;

use cephal::style::{
    GridAutoFlow, GridContainerStyle, GridPlacement, GridTemplateArea, GridTemplateAreas,
    GridTemplateComponent, GridTemplateRepetition, MaxTrackSizingFunction, MinTrackSizingFunction,
    RepetitionCount, TrackSizingFunction,
};
use cephal::{Line, geometry};
use zgui_css::computed::style::style_structs::Position;
use zgui_css::values::grid::{
    GridAutoFlowValue, GridLineValue, GridTemplateAreasValue, GridTemplateComponentValue,
    RepeatCount, TrackBreadthValue, TrackListEntry, TrackListValue, TrackSizeValue,
};
use zgui_css::values::length::LengthPercentage as CssLengthPercentage;
use zgui_interned::Ident;

use crate::style::grid::idents::IdentTable;

/// The container half, or nothing if every grid property is initial.
pub(crate) fn container(
    position: &Position,
    scale: f32,
    idents: &mut IdentTable,
) -> Option<Arc<GridContainerStyle>> {
    let auto_flow = auto_flow(position.grid_auto_flow);
    let rows = template(&position.grid_template_rows, scale);
    let columns = template(&position.grid_template_columns, scale);
    let auto_rows: Vec<_> = position
        .grid_auto_rows
        .0
        .iter()
        .map(|size| track(size, scale))
        .collect();
    let auto_columns: Vec<_> = position
        .grid_auto_columns
        .0
        .iter()
        .map(|size| track(size, scale))
        .collect();
    let areas = areas(&position.grid_template_areas, idents);
    let initial = auto_flow == GridAutoFlow::Row
        && rows.is_none()
        && columns.is_none()
        && auto_rows.len() <= 1
        && auto_rows.iter().all(|it| *it == TrackSizingFunction::AUTO)
        && auto_columns.len() <= 1
        && auto_columns
            .iter()
            .all(|it| *it == TrackSizingFunction::AUTO)
        && areas.is_none();
    if initial {
        return None;
    }
    let (template_rows, template_row_names) =
        rows.map_or_else(Default::default, |(tracks, names)| (tracks, names(idents)));
    let (template_columns, template_column_names) =
        columns.map_or_else(Default::default, |(tracks, names)| (tracks, names(idents)));
    Some(Arc::new(GridContainerStyle {
        template_rows,
        template_columns,
        template_row_names,
        template_column_names,
        auto_rows,
        auto_columns,
        auto_flow,
        template_areas: areas,
    }))
}

type Names<'a> = Box<dyn FnOnce(&mut IdentTable) -> Vec<Vec<cephal::style::Ident>> + 'a>;

/// One template's tracks and a builder for its line names, or nothing for `none`.
fn template(
    component: &GridTemplateComponentValue,
    scale: f32,
) -> Option<(Vec<GridTemplateComponent>, Names<'_>)> {
    let GridTemplateComponentValue::TrackList(list) = component else {
        return None;
    };
    let tracks = list
        .values
        .iter()
        .map(|entry| match entry {
            TrackListEntry::TrackSize(size) => GridTemplateComponent::Single(track(size, scale)),
            TrackListEntry::TrackRepeat(repeat) => {
                GridTemplateComponent::Repeat(GridTemplateRepetition {
                    count: match &repeat.count {
                        RepeatCount::Number(times) => {
                            RepetitionCount::Count(u16::try_from(*times).unwrap_or(u16::MAX))
                        }
                        RepeatCount::AutoFill => RepetitionCount::AutoFill,
                        RepeatCount::AutoFit => RepetitionCount::AutoFit,
                    },
                    tracks: repeat
                        .track_sizes
                        .iter()
                        .map(|size| track(size, scale))
                        .collect(),
                    // Names inside a repetition are not carried: what a repeated line is called
                    // depends on how many times it repeated.
                    line_names: Vec::new(),
                })
            }
        })
        .collect();
    Some((tracks, Box::new(move |idents| line_names(list, idents))))
}

fn line_names(list: &TrackListValue, idents: &mut IdentTable) -> Vec<Vec<cephal::style::Ident>> {
    list.line_names
        .iter()
        .map(|line| {
            line.iter()
                .map(|name| idents.intern(Ident::new(name.0.as_ref())))
                .collect()
        })
        .collect()
}

fn areas(value: &GridTemplateAreasValue, idents: &mut IdentTable) -> Option<GridTemplateAreas> {
    let GridTemplateAreasValue::Areas(areas) = value else {
        return None;
    };
    let inner = &areas.0;
    Some(GridTemplateAreas {
        areas: inner
            .areas
            .iter()
            .map(|area| GridTemplateArea {
                name: idents.intern(Ident::new(area.name.as_ref())),
                row_start: area.rows.start as u16,
                row_end: area.rows.end as u16,
                column_start: area.columns.start as u16,
                column_end: area.columns.end as u16,
            })
            .collect(),
        row_count: inner.strings.len() as u16,
        column_count: inner.width as u16,
    })
}

fn auto_flow(raw: GridAutoFlowValue) -> GridAutoFlow {
    let dense = raw.contains(GridAutoFlowValue::DENSE);
    match (raw.contains(GridAutoFlowValue::COLUMN), dense) {
        (false, false) => GridAutoFlow::Row,
        (false, true) => GridAutoFlow::RowDense,
        (true, false) => GridAutoFlow::Column,
        (true, true) => GridAutoFlow::ColumnDense,
    }
}

/// One track's sizing function.
pub(crate) fn track(size: &TrackSizeValue, scale: f32) -> TrackSizingFunction {
    match size {
        TrackSizeValue::Breadth(breadth) => TrackSizingFunction {
            min: min_breadth(breadth, scale),
            max: max_breadth(breadth, scale),
        },
        TrackSizeValue::Minmax(min, max) => TrackSizingFunction {
            min: min_breadth(min, scale),
            max: max_breadth(max, scale),
        },
        TrackSizeValue::FitContent(limit) => TrackSizingFunction {
            min: MinTrackSizingFunction::AUTO,
            max: fit_content(limit, scale),
        },
    }
}

/// A plain length or percentage breadth; `calc()` has no track form and reads as `auto`.
fn plain(value: &CssLengthPercentage, scale: f32) -> Option<cephal::style::Length> {
    if let Some(length) = value.to_length() {
        Some(cephal::style::Length::length(length.px() * scale))
    } else {
        value
            .to_percentage()
            .map(|percentage| cephal::style::Length::percent(percentage.0))
    }
}

fn min_breadth(breadth: &TrackBreadthValue, scale: f32) -> MinTrackSizingFunction {
    match breadth {
        TrackBreadthValue::Breadth(value) => {
            plain(value, scale).map_or(MinTrackSizingFunction::AUTO, MinTrackSizingFunction)
        }
        TrackBreadthValue::MinContent => MinTrackSizingFunction::MIN_CONTENT,
        TrackBreadthValue::MaxContent => MinTrackSizingFunction::MAX_CONTENT,
        TrackBreadthValue::Auto | TrackBreadthValue::Flex(_) => MinTrackSizingFunction::AUTO,
    }
}

fn max_breadth(breadth: &TrackBreadthValue, scale: f32) -> MaxTrackSizingFunction {
    match breadth {
        TrackBreadthValue::Breadth(value) => {
            plain(value, scale).map_or(MaxTrackSizingFunction::AUTO, MaxTrackSizingFunction)
        }
        TrackBreadthValue::MinContent => MaxTrackSizingFunction::MIN_CONTENT,
        TrackBreadthValue::MaxContent => MaxTrackSizingFunction::MAX_CONTENT,
        TrackBreadthValue::Auto => MaxTrackSizingFunction::AUTO,
        TrackBreadthValue::Flex(flex) => MaxTrackSizingFunction::fr(flex.0),
    }
}

fn fit_content(breadth: &TrackBreadthValue, scale: f32) -> MaxTrackSizingFunction {
    let TrackBreadthValue::Breadth(value) = breadth else {
        return MaxTrackSizingFunction::AUTO;
    };
    if let Some(length) = value.to_length() {
        MaxTrackSizingFunction::fit_content_px(length.px() * scale)
    } else if let Some(percentage) = value.to_percentage() {
        MaxTrackSizingFunction::fit_content_percent(percentage.0)
    } else {
        MaxTrackSizingFunction::AUTO
    }
}

/// One `grid-row` or `grid-column` placement.
pub(crate) fn placement(line: &GridLineValue, idents: &mut IdentTable) -> GridPlacement {
    let named =
        (!line.ident.0.is_empty()).then(|| idents.intern(Ident::new(line.ident.0.as_ref())));
    match (line.is_span, named) {
        (true, None) => match u16::try_from(line.line_num) {
            Ok(0) | Err(_) => GridPlacement::Span(1),
            Ok(span) => GridPlacement::Span(span),
        },
        (true, Some(name)) => {
            GridPlacement::NamedSpan(name, u16::try_from(line.line_num).unwrap_or(1).max(1))
        }
        (false, None) => match i16::try_from(line.line_num) {
            Ok(0) | Err(_) => GridPlacement::Auto,
            Ok(index) => GridPlacement::Line(index),
        },
        (false, Some(name)) => {
            GridPlacement::NamedLine(name, i16::try_from(line.line_num).unwrap_or(1))
        }
    }
}

/// Both ends of one placement.
pub(crate) fn line(
    start: &GridLineValue,
    end: &GridLineValue,
    idents: &mut IdentTable,
) -> Line<GridPlacement> {
    geometry::Line {
        start: placement(start, idents),
        end: placement(end, idents),
    }
}
