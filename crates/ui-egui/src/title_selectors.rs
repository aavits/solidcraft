//! Title-bar selectors: an Inventor-style selection priority (what a click on the model picks)
//! and the view projection (orthographic or perspective), each a drop-down after Redo.
use crate::SolveApp;
use crate::theme::Tokens;
use crate::ui_kit::{Container, Surface};
use egui::{FontId, Rect, Sense, Stroke, pos2, vec2};
use serde::{Deserialize, Serialize};
use solidcraft_engine::Sel;

/// What a click on a body's face selects. `Geometry` leaves faces, edges and vertices to the
/// SELECT panel's filter; `Body` is the panel's "Select bodies" box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickLevel {
    #[default]
    Geometry,
    Body,
    Feature,
    /// The innermost component placement holding the body.
    Part,
    /// The top-level component placement holding the body.
    Component,
}

impl PickLevel {
    pub const ALL: [PickLevel; 5] = [PickLevel::Geometry, PickLevel::Body, PickLevel::Feature, PickLevel::Part, PickLevel::Component];

    pub fn label(self) -> &'static str {
        match self {
            PickLevel::Geometry => "Faces & edges",
            PickLevel::Body => "Bodies",
            PickLevel::Feature => "Features",
            PickLevel::Part => "Parts",
            PickLevel::Component => "Components",
        }
    }

    fn key(self) -> &'static str {
        match self {
            PickLevel::Geometry => "geometry",
            PickLevel::Body => "body",
            PickLevel::Feature => "feature",
            PickLevel::Part => "part",
            PickLevel::Component => "component",
        }
    }
}

/// The priority in effect. The SELECT panel and Preferences still set `pick_bodies` on their
/// own, so Geometry and Body follow that box.
pub fn level(app: &SolveApp) -> PickLevel {
    match app.ui.pick_level {
        PickLevel::Geometry | PickLevel::Body if app.ui.pick_bodies => PickLevel::Body,
        PickLevel::Geometry | PickLevel::Body => PickLevel::Geometry,
        l => l,
    }
}

pub fn set_level(app: &mut SolveApp, l: PickLevel) {
    app.ui.pick_level = l;
    app.ui.pick_bodies = l == PickLevel::Body;
}

/// A face click picks a whole object (body, feature, part or component), not the face.
pub fn picks_whole(app: &SolveApp) -> bool {
    level(app) != PickLevel::Geometry
}

/// What a click on face `index` of `body` selects at the current priority. A face no feature
/// or placement can be traced to picks its body.
pub fn face_pick(app: &SolveApp, body: &str, index: usize) -> Option<Sel> {
    let body_sel = || Sel::Body { name: body.to_string() };
    Some(match level(app) {
        PickLevel::Geometry => return None,
        PickLevel::Body => body_sel(),
        PickLevel::Feature => face_feature(app, body, index).map(|id| Sel::Feature { id }).unwrap_or_else(body_sel),
        PickLevel::Part => occurrence_path(app, body).map(|path| Sel::Occurrence { path }).unwrap_or_else(body_sel),
        PickLevel::Component => occurrence_path(app, body)
            .map(|mut path| {
                path.truncate(1);
                Sel::Occurrence { path }
            })
            .unwrap_or_else(body_sel),
    })
}

/// The feature that made a face, from its persistent name (`F<feature>:…`), else the feature
/// that made its body. Like the context menu's Edit Feature, only features of the body's own
/// component count.
fn face_feature(app: &SolveApp, body: &str, index: usize) -> Option<u64> {
    let state = app.session.world_state();
    let b = state.body(body)?;
    let component = app.session.doc.model_body_component(b);
    let named = solidcraft_engine::doc::naming::face_names(b)
        .get(index)
        .and_then(|n| n.strip_prefix('F')?.split_once(':')?.0.parse::<u64>().ok())
        .filter(|_| {
            !app.session.doc.feature(b.feature).is_some_and(|f| {
                matches!(f.kind, solidcraft_engine::doc::FeatureKind::Import { .. } | solidcraft_engine::doc::FeatureKind::MeshImport { .. })
            })
        });
    named.into_iter().chain([b.feature]).find(|id| app.session.doc.feature(*id).is_some_and(|f| f.component == component))
}

/// The innermost placement path holding a world body (none in a design without components).
fn occurrence_path(app: &SolveApp, body: &str) -> Option<Vec<u64>> {
    let s = &app.session;
    if s.doc.occurrences.is_empty() {
        return None;
    }
    let mut stack = s.occurrence_children(&[]).ok()?;
    let mut best: Option<Vec<u64>> = None;
    let mut visited = 0;
    while let Some(p) = stack.pop() {
        visited += 1;
        if visited > 100_000 {
            break;
        }
        if p.bodies.iter().any(|b| b.world == body) && best.as_ref().is_none_or(|b| p.path.len() > b.len()) {
            best = Some(p.path.clone());
        }
        if let Ok(children) = s.occurrence_children(&p.path) {
            stack.extend(children);
        }
    }
    best
}

/// Title bars at least this wide show the selectors; from [`LABELED_FROM`] with their values.
/// Narrower ones leave the room to the document tabs (body picking stays in the ribbon's SELECT
/// panel, and perspective in the view bar and Preferences › Graphics).
pub const SHOWN_FROM: f32 = 1100.0;
pub const LABELED_FROM: f32 = 1280.0;

/// Both drop-downs, starting at `x` in the title bar `bar`; returns where they end. `labeled`
/// shows each one's current value beside its icon.
pub fn show(app: &mut SolveApp, ui: &mut egui::Ui, bar: Rect, x: f32, labeled: bool) -> f32 {
    let current = level(app);
    let (rect, response) = dropdown(ui, bar, x, "select", current.label(), labeled, "titlebar:select_priority");
    let response = response.on_hover_text(format!("Selection priority: {} (what a click on the model selects)", current.label()));
    menu(ui, &response, |ui| {
        for l in PickLevel::ALL {
            if crate::ui_kit::checkbox_row(ui, &format!("pick_level:{}", l.key()), l.label(), current == l) {
                set_level(app, l);
                ui.close();
            }
        }
    });
    let label = if app.ui.perspective { "Perspective" } else { "Orthographic" };
    let (rect, response) = dropdown(ui, bar, rect.right() + 6.0, "perspective", label, labeled, "titlebar:projection");
    let response = response.on_hover_text(format!("View projection: {label} (sketches are always drawn orthographic)"));
    menu(ui, &response, |ui| {
        for (persp, label) in [(false, "Orthographic"), (true, "Perspective")] {
            if crate::ui_kit::checkbox_row(ui, &format!("projection:{label}"), label, app.ui.perspective == persp) {
                app.ui.perspective = persp;
                ui.close();
            }
        }
    });
    rect.right()
}

/// A title-bar button: icon, current value and a chevron, drawn like the File button.
fn dropdown(ui: &mut egui::Ui, bar: Rect, x: f32, icon: &str, value: &str, labeled: bool, handle: &str) -> (Rect, egui::Response) {
    let t = Tokens::get();
    let font = FontId::proportional(t.font_button);
    let galley = ui.painter().layout_no_wrap(if labeled { value.to_string() } else { String::new() }, font, t.app_bar_text);
    let (icon_w, chevron_w) = (24.0, 16.0);
    let text_w = if labeled { galley.size().x + 6.0 } else { 0.0 };
    let rect = Rect::from_min_size(pos2(x, bar.center().y - 13.0), vec2(icon_w + text_w + chevron_w, 26.0));
    let response = ui.interact(rect, ui.id().with(("sc_title_selector", handle)), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, value));
    crate::scenario::publish_target(handle, rect);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    if response.hovered() || response.has_focus() || open {
        ui.painter().rect_filled(rect, t.radius_sm, t.app_bar_hover);
    }
    crate::icons::paint_app_bar(ui.painter(), Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(16.0, 16.0)), icon, 1.0);
    ui.painter().galley(pos2(rect.left() + icon_w, rect.center().y - galley.size().y / 2.0), galley, t.app_bar_text);
    let c = pos2(rect.right() - chevron_w / 2.0 - 2.0, rect.center().y + 1.0);
    let stroke = Stroke::new(1.3, t.app_bar_text);
    ui.painter().line_segment([pos2(c.x - 3.5, c.y - 2.0), c + vec2(0.0, 1.5)], stroke);
    ui.painter().line_segment([c + vec2(0.0, 1.5), pos2(c.x + 3.5, c.y - 2.0)], stroke);
    (rect, response)
}

fn menu(ui: &egui::Ui, response: &egui::Response, add: impl FnOnce(&mut egui::Ui)) {
    Container::new(ui.ctx(), Surface::Popup).menu(response).frame(crate::ui_kit::Menu::frame()).show(|ui| {
        // Rows fill the width they get: give them a menu's width, not the screen's.
        ui.set_width(180.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SolveApp {
        SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default())
    }

    #[test]
    fn levels_round_trip_and_body_follows_the_select_bodies_box() {
        let mut app = app();
        assert_eq!(level(&app), PickLevel::Geometry);
        // The SELECT panel's box alone still means Body.
        app.ui.pick_bodies = true;
        assert_eq!(level(&app), PickLevel::Body);
        set_level(&mut app, PickLevel::Feature);
        assert!(!app.ui.pick_bodies);
        assert_eq!(level(&app), PickLevel::Feature);
        assert!(picks_whole(&app));
        set_level(&mut app, PickLevel::Geometry);
        assert!(!picks_whole(&app));
        for l in PickLevel::ALL {
            let text = serde_json::to_string(&l).unwrap();
            assert_eq!(serde_json::from_str::<PickLevel>(&text).unwrap(), l);
        }
    }

    #[test]
    fn a_face_picks_its_feature_body_or_nothing_by_level() {
        let mut app = app();
        for (cmd, p) in [
            ("sketch.create", serde_json::json!({"plane": "XY"})),
            ("sketch.rectangle.two_point", serde_json::json!({"p0": [0, 0], "p1": [20, 10]})),
            ("sketch.finish", serde_json::json!({})),
            ("solid.extrude", serde_json::json!({"distance": 5})),
            ("solid.fillet", serde_json::json!({"edges": [[10, 0, 5]], "radius": 1})),
        ] {
            app.run(cmd, p).unwrap();
        }
        let state = app.session.world_state();
        let body = state.bodies[0].name.clone();
        let fillet = app.session.doc.features.last().unwrap().id;
        let extrude = state.bodies[0].feature;
        let names = solidcraft_engine::doc::naming::face_names(&state.bodies[0]);
        let face_of = |id: u64| names.iter().position(|n| n.starts_with(&format!("F{id}:"))).unwrap();
        assert_eq!(face_pick(&app, &body, 0), None);
        set_level(&mut app, PickLevel::Feature);
        assert_eq!(face_pick(&app, &body, face_of(fillet)), Some(Sel::Feature { id: fillet }));
        assert_eq!(face_pick(&app, &body, face_of(extrude)), Some(Sel::Feature { id: extrude }));
        set_level(&mut app, PickLevel::Body);
        assert_eq!(face_pick(&app, &body, 0), Some(Sel::Body { name: body.clone() }));
        // No components: Part and Component fall back to the body.
        set_level(&mut app, PickLevel::Component);
        assert_eq!(face_pick(&app, &body, 0), Some(Sel::Body { name: body }));
    }

    #[test]
    fn parts_pick_the_innermost_placement_and_components_the_top_one() {
        let mut app = app();
        app.run("component.create", serde_json::json!({"name": "Assembly"})).unwrap();
        app.run("component.create", serde_json::json!({"name": "Bracket"})).unwrap();
        app.run("solid.box", serde_json::json!({"length": 10, "width": 10, "height": 10})).unwrap();
        let state = app.session.world_state();
        let body = state.bodies.last().unwrap().name.clone();
        let top = app.session.occurrence_children(&[]).unwrap();
        let outer = top[0].path.clone();
        let inner = app.session.occurrence_children(&outer).unwrap()[0].path.clone();
        assert_eq!(inner.len(), 2);
        set_level(&mut app, PickLevel::Part);
        assert_eq!(face_pick(&app, &body, 0), Some(Sel::Occurrence { path: inner }));
        set_level(&mut app, PickLevel::Component);
        assert_eq!(face_pick(&app, &body, 0), Some(Sel::Occurrence { path: outer }));
    }
}
