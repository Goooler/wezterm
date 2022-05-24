use crate::termwindow::box_model::*;
use crate::termwindow::modal::Modal;
use crate::termwindow::render::{
    rgbcolor_to_window_color, BOTTOM_LEFT_ROUNDED_CORNER, BOTTOM_RIGHT_ROUNDED_CORNER,
    TOP_LEFT_ROUNDED_CORNER, TOP_RIGHT_ROUNDED_CORNER,
};
use crate::termwindow::DimensionContext;
use crate::utilsprites::RenderMetrics;
use crate::TermWindow;
use config::keyassignment::{KeyAssignment, PaneSelectArguments};
use config::{Dimension, TabBarColors};
use mux::Mux;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};

pub struct PaneSelector {
    element: RefCell<Option<Vec<ComputedElement>>>,
    labels: RefCell<Vec<String>>,
    selection: RefCell<String>,
    alphabet: String,
}

impl PaneSelector {
    pub fn new(term_window: &mut TermWindow, args: &PaneSelectArguments) -> Self {
        let alphabet = if args.alphabet.is_empty() {
            term_window.config.quick_select_alphabet.clone()
        } else {
            args.alphabet.clone()
        };
        Self {
            element: RefCell::new(None),
            labels: RefCell::new(vec![]),
            selection: RefCell::new(String::new()),
            alphabet,
        }
    }

    fn compute(
        term_window: &mut TermWindow,
        alphabet: &str,
    ) -> anyhow::Result<(Vec<ComputedElement>, Vec<String>)> {
        let font = term_window
            .fonts
            .pane_select_font()
            .expect("to resolve pane selection font");
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        let top_bar_height = if term_window.show_tab_bar && !term_window.config.tab_bar_at_bottom {
            term_window.tab_bar_pixel_height().unwrap()
        } else {
            0.
        };
        let (padding_left, padding_top) = term_window.padding_left_top();
        let border = term_window.get_os_border();
        let top_pixel_y = top_bar_height + padding_top + border.top.get() as f32;

        let panes = term_window.get_panes_to_render();
        let labels =
            crate::overlay::quickselect::compute_labels_for_alphabet(alphabet, panes.len());

        let colors = term_window
            .config
            .colors
            .as_ref()
            .and_then(|c| c.tab_bar.as_ref())
            .cloned()
            .unwrap_or_else(TabBarColors::default);

        let mut elements = vec![];
        for pos in panes {
            let caption = labels[pos.index].clone();
            let element = Element::new(&font, ElementContent::Text(caption))
                .colors(ElementColors {
                    border: BorderColor::new(
                        rgbcolor_to_window_color(colors.active_tab.bg_color).into(),
                    ),
                    bg: rgbcolor_to_window_color(colors.active_tab.bg_color).into(),
                    text: rgbcolor_to_window_color(colors.active_tab.fg_color).into(),
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(0.25),
                    right: Dimension::Cells(0.25),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                })
                .border(BoxDimension::new(Dimension::Pixels(1.)))
                .border_corners(Some(Corners {
                    top_left: SizedPoly {
                        width: Dimension::Cells(0.25),
                        height: Dimension::Cells(0.25),
                        poly: TOP_LEFT_ROUNDED_CORNER,
                    },
                    top_right: SizedPoly {
                        width: Dimension::Cells(0.25),
                        height: Dimension::Cells(0.25),
                        poly: TOP_RIGHT_ROUNDED_CORNER,
                    },
                    bottom_left: SizedPoly {
                        width: Dimension::Cells(0.25),
                        height: Dimension::Cells(0.25),
                        poly: BOTTOM_LEFT_ROUNDED_CORNER,
                    },
                    bottom_right: SizedPoly {
                        width: Dimension::Cells(0.25),
                        height: Dimension::Cells(0.25),
                        poly: BOTTOM_RIGHT_ROUNDED_CORNER,
                    },
                }));

            let dimensions = term_window.dimensions;
            let pane_dims = pos.pane.get_dimensions();

            let computed = term_window.compute_element(
                &LayoutContext {
                    height: DimensionContext {
                        dpi: dimensions.dpi as f32,
                        pixel_max: dimensions.pixel_height as f32,
                        pixel_cell: metrics.cell_size.height as f32,
                    },
                    width: DimensionContext {
                        dpi: dimensions.dpi as f32,
                        pixel_max: dimensions.pixel_width as f32,
                        pixel_cell: metrics.cell_size.width as f32,
                    },
                    bounds: euclid::rect(
                        padding_left
                            + ((pos.left as f32 + pane_dims.cols as f32 / 2.)
                                * term_window.render_metrics.cell_size.width as f32),
                        top_pixel_y
                            + ((pos.top as f32 + pane_dims.viewport_rows as f32 / 2.)
                                * term_window.render_metrics.cell_size.height as f32),
                        pane_dims.cols as f32 * term_window.render_metrics.cell_size.width as f32,
                        pane_dims.viewport_rows as f32
                            * term_window.render_metrics.cell_size.height as f32,
                    ),
                    metrics: &metrics,
                    gl_state: term_window.render_state.as_ref().unwrap(),
                },
                &element,
            )?;
            elements.push(computed);
        }

        Ok((elements, labels))
    }
}

impl Modal for PaneSelector {
    fn perform_assignment(
        &self,
        _assignment: &KeyAssignment,
        _term_window: &mut TermWindow,
    ) -> bool {
        false
    }

    fn mouse_event(&self, _event: MouseEvent, _term_window: &mut TermWindow) -> anyhow::Result<()> {
        Ok(())
    }

    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<()> {
        match (key, mods) {
            (KeyCode::Escape, KeyModifiers::NONE) => {
                term_window.cancel_modal();
            }
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                // Type to add to the selection
                let mut selection = self.selection.borrow_mut();
                selection.push(c);

                // and if we have a complete match, activate that pane
                if let Some(pane_index) = self.labels.borrow().iter().position(|s| s == &*selection)
                {
                    let mux = Mux::get().unwrap();
                    let tab = match mux.get_active_tab_for_window(term_window.mux_window_id) {
                        Some(tab) => tab,
                        None => return Ok(()),
                    };

                    let tab_id = tab.tab_id();

                    if term_window.tab_state(tab_id).overlay.is_none() {
                        let panes = tab.iter_panes();
                        if panes.iter().position(|p| p.index == pane_index).is_some() {
                            tab.set_active_idx(pane_index);
                        }
                    }

                    term_window.cancel_modal();
                }
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                // Backspace to edit the selection
                let mut selection = self.selection.borrow_mut();
                selection.pop();
            }
            (KeyCode::Char('u'), KeyModifiers::CTRL) => {
                // CTRL-u to clear the selection
                let mut selection = self.selection.borrow_mut();
                selection.clear();
            }
            _ => {}
        }
        Ok(())
    }

    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<[ComputedElement]>> {
        if self.element.borrow().is_none() {
            let (element, labels) = Self::compute(term_window, &self.alphabet)?;
            self.element.borrow_mut().replace(element);
            *self.labels.borrow_mut() = labels;
        }
        Ok(Ref::map(self.element.borrow(), |v| {
            v.as_ref().unwrap().as_slice()
        }))
    }

    fn reconfigure(&self, _term_window: &mut TermWindow) {
        self.element.borrow_mut().take();
    }
}
