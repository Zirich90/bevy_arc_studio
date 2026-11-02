//! An optional default look (feature `default_style`), opted into per entity:
//!
//! | Add                               | To get                                          |
//! |-----------------------------------|-------------------------------------------------|
//! | [`EdgeStyle`] on a canvas or edge | Bézier wires (edges and the dragged wire)       |
//! | [`CanvasGrid`] on a canvas        | a pannable, zoomable grid                       |
//! | [`SelectionBoxStyle`] on a canvas | a visible selection box                         |
//! | [`PortHighlight`] + [`PortColor`] | ports reflecting connection and drag state      |
//! | [`SelectedBorderColor`] on a node | a border following `Selected`                   |
//!
//! [`kit`] has ready-made canvas and node scenes to compose with Bevy's `bsn!`.

pub mod kit;
mod render;

use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::cursor::{EntityCursor, OverrideCursor};
use bevy::picking::hover::PickingInteraction;
use bevy::picking::{Pickable, PickingSystems};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, Selected};
use bevy::window::SystemCursorIcon;

use crate::interaction::{SelectionBox, WireCandidates, WireTarget};
use crate::{NoodleSystems, components::*, query::GraphQuery};
use render::{Grid, GridMaterial, MaterialsPlugin, WireMaterial, wire_material};

/// The optional default look: wires, grids, selection boxes, port and selection highlights. Each piece is opted into per entity.
///
/// The cursors it puts on nodes, ports and dragged wires need Bevy's
/// [`CursorIconPlugin`](bevy::picking::cursor::CursorIconPlugin), which the
/// app adds (Feathers adds it too). Without it the graph works, with the
/// default cursor.
pub struct NoodleDefaultStylePlugin;

impl Plugin for NoodleDefaultStylePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .add_plugins(MaterialsPlugin)
            .add_systems(
                PostUpdate,
                (
                    draw_edges,
                    draw_grids,
                    draw_selection_boxes,
                    highlight_ports,
                    selected_borders,
                    outline_focus,
                )
                    .in_set(NoodleSystems::Render),
            )
            .add_systems(PreUpdate, wire_cursor.after(PickingSystems::Last));
    }
}

/// How edges are drawn. On a canvas it applies to all its edges; on an edge
/// it overrides the canvas. Lengths are in graph units.
///
/// ```
/// use bevy::prelude::*;
/// use bevy_noodle::style::EdgeStyle;
/// // Marching ants flowing from output to input, fading blue to pink.
/// let style = EdgeStyle {
///     color: Some(Color::srgb(0.3, 0.6, 1.0)),
///     end_color: Some(Color::srgb(1.0, 0.4, 0.7)),
///     dash: Some(Vec2::new(10.0, 6.0)),
///     flow_speed: 40.0,
///     ..default()
/// };
/// ```
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct EdgeStyle {
    /// `None` uses the output port's [`PortColor`].
    pub color: Option<Color>,
    /// Fade to this color toward the input end. `None`: no gradient.
    pub end_color: Option<Color>,
    /// Stroke width, in graph units.
    pub width: f32,
    /// How far the curve's handles reach, as a fraction of the distance between ends.
    pub curvature: f32,
    /// Dash and gap lengths. `None`: a solid wire.
    pub dash: Option<Vec2>,
    /// Animate dashes toward the input at this speed (per second). Solid wires
    /// carry travelling pulses instead. Negative flows backward.
    pub flow_speed: f32,
    /// Draw wires under the nodes (then only pickable over empty canvas)
    /// instead of above them.
    pub below_nodes: bool,
    /// End wires at port rims instead of centers.
    pub trim_to_ports: bool,
    /// The color of a [`Selected`] edge. `None`: unchanged.
    pub selected_color: Option<Color>,
    /// Extra width while the pointer is over the edge.
    pub hover_width: f32,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: None,
            end_color: None,
            width: 3.0,
            curvature: 0.5,
            dash: None,
            flow_speed: 0.0,
            below_nodes: false,
            trim_to_ports: true,
            selected_color: Some(Color::srgb_u8(250, 204, 92)),
            hover_width: 2.0,
        }
    }
}

/// A port's color, used by [`PortHighlight`] and as the default wire color.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Deref)]
#[reflect(Component, Default)]
pub struct PortColor(pub Color);

impl Default for PortColor {
    fn default() -> Self {
        Self(Color::srgb_u8(160, 164, 172))
    }
}

/// Makes a port's background, border and scale follow its state.
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
#[reflect(Component, Default)]
#[require(UiTransform)]
pub struct PortHighlight;

/// A pannable, zoomable background grid for a canvas.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct CanvasGrid {
    /// Minor line spacing in graph units.
    pub spacing: f32,
    /// Every how many minor lines a major one is drawn.
    pub major_every: u32,
    /// Minor line color.
    pub minor: Color,
    /// Major line color.
    pub major: Color,
    /// Fill behind the lines.
    pub background: Color,
}

impl Default for CanvasGrid {
    fn default() -> Self {
        // UI blends in linear space, so small alphas already read clearly.
        let line = |alpha| Color::srgba(1.0, 1.0, 1.0, alpha);
        Self {
            spacing: 24.0,
            major_every: 5,
            minor: line(0.012),
            major: line(0.028),
            background: Color::NONE,
        }
    }
}

/// How the selection box looks while box-selecting.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct SelectionBoxStyle {
    /// Fill color.
    pub fill: Color,
    /// Border color.
    pub border: Color,
}

impl Default for SelectionBoxStyle {
    fn default() -> Self {
        Self {
            fill: Color::srgba(0.43, 0.59, 0.86, 0.12),
            border: Color::srgba(0.43, 0.59, 0.86, 0.8),
        }
    }
}

/// On a canvas: an outline on its node or port with keyboard focus (see
/// [`CanvasKeyboard`](crate::CanvasKeyboard)).
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct FocusOutline {
    /// Outline color.
    pub color: Color,
    /// Outline width, in logical pixels.
    pub width: f32,
}

impl Default for FocusOutline {
    fn default() -> Self {
        Self {
            color: Color::srgb_u8(120, 180, 255),
            width: 2.0,
        }
    }
}

/// Sets `BorderColor` from whether the entity is [`Selected`].
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct SelectedBorderColor {
    /// Border color when not selected.
    pub normal: Color,
    /// Border color when selected.
    pub selected: Color,
}

impl Default for SelectedBorderColor {
    fn default() -> Self {
        Self {
            normal: Color::srgb_u8(66, 69, 77),
            selected: Color::srgb_u8(250, 204, 92),
        }
    }
}

/// Library-owned visuals, linked from what they draw.
#[derive(Component)]
struct GridVisual(Entity);
#[derive(Component)]
struct BoxVisual(Entity);

/// On a wire's UI node: the edge (or pending wire) it draws. The node is a
/// sibling under the canvas content, so the edge itself stays a plain entity
/// that pointer events can reach, and it is despawned with the edge.
#[derive(Component)]
#[relationship(relationship_target = EdgeVisual)]
pub(crate) struct DrawsEdge(Entity);

#[derive(Component)]
#[relationship_target(relationship = DrawsEdge, linked_spawn)]
pub(crate) struct EdgeVisual(Vec<Entity>);

/// Shows `node` at `rect` (local pixels of its parent), or hides it.
fn place(node: &mut Node, rect: Option<Rect>) {
    node.position_type = PositionType::Absolute;
    node.display = if rect.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let rect = rect.unwrap_or_default();
    (node.left, node.top) = (px(rect.min.x), px(rect.min.y));
    (node.width, node.height) = (px(rect.width()), px(rect.height()));
}

/// Replaces an asset only when it changed, so unchanged frames upload nothing.
fn update<M: Asset + PartialEq>(assets: &mut Assets<M>, handle: &Handle<M>, value: M) {
    if assets.get(handle) != Some(&value)
        && let Some(mut current) = assets.get_mut(handle)
    {
        *current = value;
    }
}

fn draw_edges(
    mut commands: Commands,
    graph: GraphQuery,
    canvases: Query<Option<&EdgeStyle>, With<NodeCanvas>>,
    mut edges: Query<
        (
            Entity,
            Option<&EdgeGeometry>,
            Option<&EdgeStyle>,
            Option<&WireOf>,
            Option<&EdgeVisual>,
            Option<&mut EdgeHitbox>,
            Has<Selected>,
            Option<&PickingInteraction>,
        ),
        Or<(With<Edge>, With<PendingWire>)>,
    >,
    mut visuals: Query<(
        &mut Node,
        &mut ZIndex,
        &MaterialNode<WireMaterial>,
        &ChildOf,
    )>,
    ports: Query<(Option<&PortColor>, &ComputedNode, Option<&UiTransform>)>,
    mut materials: ResMut<Assets<WireMaterial>>,
) {
    for (entity, geometry, own, wire, visual, hitbox, selected, pointer) in &mut edges {
        let visual = visual.and_then(|v| v.0.first().copied());
        let canvas = wire.map(|w| w.0).or_else(|| graph.canvas_of(entity));
        let style = own.or(canvas.and_then(|c| canvases.get(c).ok().flatten()));
        let content = canvas.and_then(|c| graph.content_of(c));
        // Not laid out (yet, or any more) or not styled: hidden and not pickable.
        let (Some(geometry), Some(style), Some(content)) = (geometry, style, content) else {
            if let Some((mut node, ..)) = visual.and_then(|v| visuals.get_mut(v).ok()) {
                place(&mut node, None);
            }
            if hitbox.is_some() {
                commands.entity(entity).try_remove::<EdgeHitbox>();
            }
            continue;
        };
        let (start, end) = (geometry.output, geometry.input);
        let port = |p: Option<Entity>| p.and_then(|p| ports.get(p).ok());
        let radius = |p| {
            port(p).map_or(0.0, |(_, computed, transform)| {
                let scale = transform.map_or(1.0, |t| t.scale.x);
                computed.size().min_element() * computed.inverse_scale_factor() * scale / 2.0
            })
        };
        let mut shape = *geometry;
        if style.trim_to_ports {
            shape.start += shape.start_tangent * radius(start);
            shape.end += shape.end_tangent * radius(end);
        }
        let points = shape.bezier(style.curvature);
        let port_color = [start, end]
            .into_iter()
            .find_map(|p| port(p)?.0.map(|c| c.0));
        let color = style
            .color
            .or(port_color)
            .unwrap_or(Color::srgb(0.8, 0.82, 0.86));
        let mut colors = [color, style.end_color.unwrap_or(color)];
        if let (true, Some(highlight)) = (selected, style.selected_color) {
            colors = [highlight; 2];
        }
        let alpha = if wire.is_some() { 0.85 } else { 1.0 };
        let colors = colors.map(|c| c.with_alpha(c.alpha() * alpha));
        let hovered = pointer.is_some_and(|p| *p != PickingInteraction::None);
        let width = style.width + if hovered { style.hover_width } else { 0.0 };
        let pattern = style
            .dash
            .unwrap_or_default()
            .max(Vec2::ZERO)
            .extend(style.flow_speed);
        let (rect, material) = wire_material(points, colors, width, pattern);
        let z = match (wire, style.below_nodes) {
            (Some(_), _) => ZIndex(i32::MAX),
            (None, false) => ZIndex(i32::MAX - 1),
            (None, true) => ZIndex(-1),
        };
        if wire.is_none() {
            let radius = style.width / 2.0 + 3.0;
            let area = EdgeHitbox {
                points,
                radius,
                below_nodes: style.below_nodes,
            };
            match hitbox {
                Some(mut hitbox) => _ = hitbox.set_if_neq(area),
                None => _ = commands.entity(entity).try_insert(area),
            }
        }
        // Wires are drawn by their own UI node, so the edge itself can be picked.
        let shown = Some(rect);
        let Some((visual, (mut node, mut z_index, handle, parent))) =
            visual.and_then(|v| Some((v, visuals.get_mut(v).ok()?)))
        else {
            // Placed right away, so a new wire shows in the frame it appears.
            let mut node = Node::default();
            place(&mut node, shown);
            let wire = (
                MaterialNode(materials.add(material)),
                DrawsEdge(entity),
                ChildOf(content),
            );
            commands.spawn((node, z, Pickable::IGNORE, wire));
            continue;
        };
        place(&mut node, shown);
        update(&mut materials, &handle.0, material);
        z_index.set_if_neq(z);
        if parent.parent() != content {
            commands.entity(visual).try_insert(ChildOf(content));
        }
    }
}

fn draw_grids(
    mut commands: Commands,
    canvases: Query<(
        Entity,
        &CanvasView,
        &ComputedNode,
        Option<&CanvasGrid>,
        Option<&GridVisual>,
    )>,
    grids: Query<&MaterialNode<GridMaterial>>,
    mut materials: ResMut<Assets<GridMaterial>>,
) {
    for (canvas, view, computed, grid, visual) in &canvases {
        let Some(grid) = grid else {
            if let Some(visual) = visual {
                commands.entity(visual.0).try_despawn();
                commands.entity(canvas).try_remove::<GridVisual>();
            }
            continue;
        };
        let size = (computed.size() * computed.inverse_scale_factor()).max(Vec2::ONE);
        let linear = |c: Color| c.to_linear().to_vec4();
        let grid = Grid {
            background: linear(grid.background),
            minor: linear(grid.minor),
            major: linear(grid.major),
            view: Vec4::new(view.pan.x, view.pan.y, view.zoom, grid.spacing.max(1.0)),
            extent: Vec4::new(size.x, size.y, grid.major_every.max(1) as f32, 0.0),
        };
        let material = GridMaterial { grid };
        match visual.and_then(|v| grids.get(v.0).ok()) {
            Some(handle) => update(&mut materials, &handle.0, material),
            None => {
                let (width, height) = (percent(100), percent(100));
                let fill = Node {
                    position_type: PositionType::Absolute,
                    width,
                    height,
                    ..default()
                };
                let fill = (fill, MaterialNode(materials.add(material)), ZIndex(-1));
                let child = commands
                    .spawn((fill, Pickable::IGNORE, ChildOf(canvas)))
                    .id();
                commands.entity(canvas).try_insert(GridVisual(child));
            }
        }
    }
}

fn draw_selection_boxes(
    mut commands: Commands,
    canvases: Query<(
        Entity,
        &SelectionBoxStyle,
        Option<&SelectionBox>,
        Option<&BoxVisual>,
    )>,
    mut nodes: Query<&mut Node>,
) {
    for (canvas, style, selection, visual) in &canvases {
        let rect = selection.map(|s| s.0);
        if let Some(mut node) = visual.and_then(|v| nodes.get_mut(v.0).ok()) {
            place(&mut node, rect);
            continue;
        }
        let mut node = Node {
            border: UiRect::all(px(1)),
            ..default()
        };
        place(&mut node, rect);
        let colors = (BackgroundColor(style.fill), BorderColor::all(style.border));
        let visual = (node, colors, ZIndex(i32::MAX), Pickable::IGNORE);
        let child = commands.spawn((visual, ChildOf(canvas))).id();
        commands.entity(canvas).try_insert(BoxVisual(child));
    }
}

fn highlight_ports(
    mut ports: Query<
        (
            Entity,
            &PortColor,
            Option<&OutgoingEdges>,
            Option<&IncomingEdges>,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut UiTransform,
        ),
        With<PortHighlight>,
    >,
    wires: Query<(&PendingWire, &WireTarget, Option<&WireCandidates>)>,
) {
    let sources: Vec<Entity> = wires.iter().map(|(w, ..)| w.from).collect();
    for (port, color, outgoing, incoming, mut fill, mut border, mut transform) in &mut ports {
        let target = wires.iter().any(|(_, t, _)| t.0 == Some(port));
        let candidate = wires
            .iter()
            .any(|(.., c)| c.is_some_and(|c| c.contains(&port)));
        let connected =
            outgoing.is_some_and(|e| !e.is_empty()) || incoming.is_some_and(|e| !e.is_empty());
        let (alpha, scale) = match (
            target || sources.contains(&port),
            candidate,
            sources.is_empty(),
        ) {
            (true, ..) => (1.0, 1.4),
            (_, true, _) => (1.0, 1.2),
            (_, _, false) => (0.3, 0.85),
            _ => (1.0, 1.0),
        };
        let inner = if connected {
            color.0
        } else {
            color.0.darker(0.35)
        };
        fill.set_if_neq(BackgroundColor(inner.with_alpha(inner.alpha() * alpha)));
        border.set_if_neq(BorderColor::all(color.0.with_alpha(alpha)));
        if transform.scale != Vec2::splat(scale) {
            transform.scale = Vec2::splat(scale);
        }
    }
}

fn selected_borders(mut nodes: Query<(&SelectedBorderColor, Has<Selected>, &mut BorderColor)>) {
    for (colors, selected, mut border) in &mut nodes {
        let color = if selected {
            colors.selected
        } else {
            colors.normal
        };
        border.set_if_neq(BorderColor::all(color));
    }
}

/// A crosshair while a wire is dragged, wherever the pointer is: Bevy's
/// [`OverrideCursor`], set and cleared only by this system. Without Bevy's
/// cursor plugin there is no override to set.
fn wire_cursor(
    wires: Query<(), With<PendingWire>>,
    cursor: Option<ResMut<OverrideCursor>>,
    mut ours: Local<bool>,
) {
    let Some(mut cursor) = cursor else {
        return;
    };
    let crosshair = EntityCursor::System(SystemCursorIcon::Crosshair);
    match (wires.is_empty(), *ours) {
        (false, false) if cursor.0.is_none() => {
            cursor.0 = Some(crosshair);
            *ours = true;
        }
        (true, true) => {
            if cursor.0 == Some(crosshair) {
                cursor.0 = None;
            }
            *ours = false;
        }
        _ => {}
    }
}

/// Keyboard focus on a node or port of a canvas with [`FocusOutline`] shows it.
fn outline_focus(
    focus: Res<InputFocus>,
    visible: Res<InputFocusVisible>,
    graph: GraphQuery,
    styles: Query<&FocusOutline>,
    mut shown: Local<Option<Entity>>,
    mut commands: Commands,
) {
    if !focus.is_changed() && !visible.is_changed() {
        return;
    }
    if let Some(mut old) = shown.take().and_then(|e| commands.get_entity(e).ok()) {
        old.try_remove::<Outline>();
    }
    let Some(entity) = focus.get().filter(|_| visible.0) else {
        return;
    };
    let style = graph.canvas_of(entity).and_then(|c| styles.get(c).ok());
    let item = graph.node_of(entity) == Some(entity) || graph.port(entity).is_some();
    if let (Some(style), true) = (style, item) {
        let outline = Outline::new(px(style.width), px(2), style.color);
        commands.entity(entity).try_insert(outline);
        *shown = Some(entity);
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use bevy::picking::cursor::CursorIconPlugin;

    #[test]
    fn edges_without_a_style_are_not_pickable() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<WireMaterial>()
            .add_systems(Update, draw_edges);
        let w = app.world_mut();
        let canvas = w.spawn((NodeCanvas, Node::default())).id();
        let port = |w: &mut World, port: Port| {
            let node = w.spawn((GraphNode, ChildOf(canvas))).id();
            w.spawn((port, ChildOf(node))).id()
        };
        let (from, to) = (
            port(w, Port::output(PortType::ANY)),
            port(w, Port::input(PortType::ANY)),
        );
        let geometry = EdgeGeometry::between((Vec2::ZERO, Vec2::X), (Vec2::X, Vec2::NEG_X));
        let edge = (
            Edge,
            EdgeSource(from),
            EdgeTarget(to),
            geometry,
            EdgeStyle::default(),
        );
        let edge = w.spawn(edge).id();
        app.update();
        assert!(app.world().get::<EdgeHitbox>(edge).is_some());
        app.world_mut().entity_mut(edge).remove::<EdgeStyle>();
        app.update();
        assert!(app.world().get::<EdgeHitbox>(edge).is_none());
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            crate::NoodleCorePlugin,
        ))
        .init_asset::<bevy::shader::Shader>();
        app
    }

    fn styled_app() -> App {
        let mut app = app();
        app.add_plugins(NoodleDefaultStylePlugin);
        app
    }

    /// Feathers adds `CursorIconPlugin` unconditionally, before or after us.
    #[test]
    fn cursor_plugin_order_does_not_matter() {
        let mut after = styled_app();
        after.add_plugins(CursorIconPlugin);
        let mut before = app();
        before.add_plugins((CursorIconPlugin, NoodleDefaultStylePlugin));
        for mut app in [after, before] {
            app.update();
            assert!(app.world().contains_resource::<OverrideCursor>());
        }
    }

    #[test]
    fn dragging_a_wire_works_without_cursor_support() {
        let mut app = styled_app();
        spawn_wire(app.world_mut());
        app.update();
        assert!(!app.world().contains_resource::<OverrideCursor>());
    }

    fn spawn_wire(w: &mut World) -> Entity {
        let canvas = w.spawn((NodeCanvas, Node::default())).id();
        let node = w.spawn((GraphNode, ChildOf(canvas))).id();
        let from = w.spawn((Port::output(PortType::ANY), ChildOf(node))).id();
        let pointer = Vec2::ZERO;
        w.spawn((PendingWire { from, pointer }, WireOf(canvas)))
            .id()
    }

    #[test]
    fn dragging_a_wire_keeps_another_systems_override() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::NoodleCorePlugin))
            .add_systems(Update, wire_cursor);
        let wait = Some(EntityCursor::System(SystemCursorIcon::Wait));
        app.insert_resource(OverrideCursor(wait.clone()));
        let wire = spawn_wire(app.world_mut());
        app.update();
        assert_eq!(app.world().resource::<OverrideCursor>().0, wait);
        app.world_mut().despawn(wire);
        app.update();
        assert_eq!(app.world().resource::<OverrideCursor>().0, wait);
    }

    #[test]
    fn dragging_a_wire_overrides_the_cursor_until_it_ends() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::NoodleCorePlugin))
            .init_resource::<OverrideCursor>()
            .add_systems(Update, wire_cursor);
        let wire = spawn_wire(app.world_mut());
        app.update();
        let crosshair = Some(EntityCursor::System(SystemCursorIcon::Crosshair));
        assert_eq!(app.world().resource::<OverrideCursor>().0, crosshair);
        app.world_mut().despawn(wire);
        app.update();
        assert_eq!(app.world().resource::<OverrideCursor>().0, None);
    }
}
