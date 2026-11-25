//! Experimental desktop cutout rig. Joint coordinates are in artwork pixels.
use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};
use procraft_core::CanvasSession;

struct Bone { layer: u64, name: String, parent: Option<usize>, joint: Pos2, tip: Pos2 }
struct Drag { bone: usize, last: f32, total: f32 }
#[derive(Default)]
pub struct Rig { enabled: bool, pose: bool, bones: Vec<Bone>, selected: Option<usize>, drag: Option<Drag>, message: String }
fn rotated(p: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let d = p-origin; let (s,c)=angle.sin_cos(); origin+vec2(c*d.x-s*d.y,s*d.x+c*d.y)
}
fn descendant(bones: &[Bone], mut i: usize, root: usize) -> bool {
    while let Some(b)=bones.get(i) { if i==root {return true;} let Some(p)=b.parent else {break;}; if p>=i {break;} i=p; } false
}
impl Rig {
    pub fn show(&mut self, ctx: &egui::Context, session: &CanvasSession, rect: Rect, allowed: bool) {
        if !allowed { if self.drag.take().is_some() {session.transform_end(false);} self.enabled=false; return; }
        let view=session.view_matrix();
        let Ok([a,b,c,d,e,f])= <[f32;6]>::try_from(view.as_slice()) else {return;};
        let det=a*d-b*c; if det.abs()<1e-8 {return;}
        let scale=ctx.pixels_per_point();
        let screen=|p:Pos2| pos2(a*p.x+c*p.y+e,b*p.x+d*p.y+f)/scale;
        let canvas=|p:Pos2| {let x=p.x*scale-e;let y=p.y*scale-f;pos2((d*x-c*y)/det,(-b*x+a*y)/det)};
        if self.enabled {
            let painter=ctx.layer_painter(egui::LayerId::new(egui::Order::Middle,egui::Id::new("rig skeleton"))).with_clip_rect(rect);
            for (i,bone) in self.bones.iter().enumerate() {
                let joint=screen(bone.joint);let tip=screen(bone.tip);
                let color=if self.selected==Some(i){Color32::YELLOW}else{Color32::from_rgb(30,220,205)};
                painter.line_segment([joint,tip],egui::Stroke::new(3.0,color));
                if let Some(parent)=bone.parent.and_then(|p|self.bones.get(p)){painter.line_segment([screen(parent.joint),joint],egui::Stroke::new(1.0,color));}
                painter.circle_filled(joint,6.0,color);painter.circle_stroke(tip,7.0,egui::Stroke::new(2.0,color));
            }
            // Only the handles claim pointer input; ordinary canvas navigation remains available.
            let mut finish=false;
            for i in 0..self.bones.len() {
                for tip in [false,true] {
                    let Some(bone)=self.bones.get(i) else {continue;};
                    let point=screen(if tip {bone.tip}else{bone.joint});
                    if !rect.contains(point) {continue;}
                    egui::Area::new(egui::Id::new(("rig handle",i,tip))).order(egui::Order::Middle).fixed_pos(point-Vec2::splat(10.0)).show(ctx,|ui|{
                        let (_,r)=ui.allocate_exact_size(Vec2::splat(20.0),egui::Sense::click_and_drag());
                        if r.clicked() || r.drag_started(){self.selected=Some(i);}
                        if !self.pose {
                            if r.dragged() && let Some(p)=r.interact_pointer_pos() && let Some(b)=self.bones.get_mut(i) {
                                let p=canvas(p); if tip {b.tip=p;} else {let delta=p-b.joint;b.joint=p;b.tip+=delta;}
                            }
                        } else if tip {
                            if r.drag_started() && self.drag.is_none() && !session.has_selection() && let Some(b)=self.bones.get(i) {
                                if session.select_layer(b.layer) && session.transform_begin().is_some() {
                                    let v=b.tip-b.joint; self.drag=Some(Drag{bone:i,last:v.y.atan2(v.x),total:0.0});
                                } else {self.message="Cannot transform this layer (empty, hidden or locked).".into();}
                            }
                            if r.dragged() && let Some(p)=r.interact_pointer_pos() && let Some(drag)=self.drag.as_mut().filter(|d|d.bone==i) && let Some(b)=self.bones.get(i) {
                                let v=canvas(p)-b.joint;if v.length_sq()>0.01 {let angle=v.y.atan2(v.x);let delta=(angle-drag.last).sin().atan2((angle-drag.last).cos());
                                if session.rig_rotate_about(b.joint.x,b.joint.y,delta){drag.last=angle;drag.total+=delta;}}
                            }
                            if r.drag_stopped() && self.drag.as_ref().is_some_and(|d|d.bone==i){finish=true;}
                        }
                    });
                }
            }
            if finish || (self.drag.is_some() && !ctx.input(|i|i.pointer.primary_down())) {
                if let Some(drag)=self.drag.take() {
                    session.transform_end(true);
                    if let Some(root)=self.bones.get(drag.bone) {
                        let origin=root.joint;let root_layer=root.layer;
                        for i in 0..self.bones.len() {
                            if !descendant(&self.bones,i,drag.bone){continue;}
                            let Some(b)=self.bones.get_mut(i) else {continue;};
                            if i!=drag.bone {
                                if !session.select_layer(b.layer) || session.transform_begin().is_none(){self.message=format!("Skipped {}: layer unavailable",b.name);continue;}
                                let ok=session.rig_rotate_about(origin.x,origin.y,drag.total);session.transform_end(ok);if !ok{continue;}
                            }
                            b.joint=rotated(b.joint,origin,drag.total);b.tip=rotated(b.tip,origin,drag.total);
                        }
                        session.select_layer(root_layer);
                    }
                }
            }
        }
        egui::Window::new("Experimental Rig").default_pos(pos2(75.0,95.0)).default_width(220.0).collapsible(true).show(ctx,|ui|{
            if ui.checkbox(&mut self.enabled,"Enable rig").changed() && !self.enabled && self.drag.take().is_some(){session.transform_end(false);}
            if !self.enabled{return;}
            ui.add_enabled_ui(self.drag.is_none(),|ui|{
                ui.checkbox(&mut self.pose,"Pose mode (drag tip to rotate)");
                ui.label(if self.pose {"Drag the hollow joint. Children follow on release."}else{"Drag solid pivot or hollow tip to place joints."});
                if ui.button("Bind current layer").clicked(){
                    if let Some(layer)=session.layers().into_iter().find(|l|l.current && l.visible && !l.locked && !l.text && l.plugin_kind.is_none() && matches!(l.kind,procraft_core::LayerType::Paint)) {
                        if self.bones.iter().any(|b|b.layer==layer.id){self.message="Layer already bound".into();}
                        else {let joint=canvas(rect.center());let tip=canvas(rect.center()+vec2(0.0,65.0));self.bones.push(Bone{layer:layer.id,name:layer.name,parent:self.selected,joint,tip});self.selected=Some(self.bones.len()-1);self.message.clear();}
                    } else {self.message="Select an unlocked visible paint layer".into();}
                }
                if ui.button("Clear parent selection").clicked(){self.selected=None;}
                for (i,b) in self.bones.iter().enumerate(){if ui.selectable_label(self.selected==Some(i),format!("{}: {} (parent {:?})",i+1,b.name,b.parent.map(|p|p+1))).clicked(){self.selected=Some(i);}}
                if ui.button("Clear rig (keeps artwork)").clicked(){self.bones.clear();self.selected=None;}
            });
            ui.label(&self.message);
        });
    }
}
