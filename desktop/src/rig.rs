//! Experimental desktop cutout rig. Joint coordinates are in artwork pixels.
use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};
use procraft_core::CanvasSession;

#[derive(Clone)]
struct Bone { layer: u64, name: String, parent: Option<usize>, joint: Pos2, tip: Pos2 }
struct Drag { bone: usize, last: f32, total: f32 }
#[derive(Default)]
pub struct Rig { enabled: bool, pose: bool, bones: Vec<Bone>, selected: Option<usize>, drag: Option<Drag>, message: String, animation: Option<Animation> }
fn rotated(p: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let d = p-origin; let (s,c)=angle.sin_cos(); origin+vec2(c*d.x-s*d.y,s*d.x+c*d.y)
}
fn descendant(bones: &[Bone], mut i: usize, root: usize) -> bool {
    while let Some(b)=bones.get(i) { if i==root {return true;} let Some(p)=b.parent else {break;}; if p>=i {break;} i=p; } false
}
impl Rig {
    pub fn show(&mut self, ctx: &egui::Context, session: &CanvasSession, rect: Rect, allowed: bool) {
        if !allowed { self.animation=None; if self.drag.take().is_some() {session.transform_end(false);} self.enabled=false; return; }
        let view=session.view_matrix();
        let Ok([a,b,c,d,e,f])= <[f32;6]>::try_from(view.as_slice()) else {return;};
        let det=a*d-b*c; if det.abs()<1e-8 {return;}
        let scale=ctx.pixels_per_point();
        let screen=|p:Pos2| pos2(a*p.x+c*p.y+e,b*p.x+d*p.y+f)/scale;
        let canvas=|p:Pos2| {let x=p.x*scale-e;let y=p.y*scale-f;pos2((d*x-c*y)/det,(-b*x+a*y)/det)};
        if self.enabled && self.animation.is_none() {
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
            if self.animation.is_none() && self.drag.is_none() && !self.bones.is_empty() && ui.button("Open Rig Animation").clicked() {
                match Animation::capture(ctx, session, &self.bones) { Ok(a)=>self.animation=Some(a), Err(e)=>self.message=e }
            }
            ui.add_enabled_ui(self.drag.is_none() && self.animation.is_none(),|ui|{
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
        if !self.enabled { self.animation=None; }
        if let Some(a)=self.animation.as_mut() { if !a.show(ctx) { self.animation=None; } }
    }
}


// The animation owns immutable textures; preview never writes to the document.
struct Sprite { bone: usize, texture: egui::TextureHandle, min: Pos2, max: Pos2, opacity: f32 }
struct Animation {
    bones: Vec<Bone>, sprites: Vec<Sprite>, size: Vec2,
    keys: std::collections::BTreeMap<u32, Vec<f32>>, angles: Vec<f32>,
    frame: u32, length: u32, fps: u32, playing: bool, clock: f64, start_frame: u32, looping: bool,
}
impl Animation {
    fn capture(ctx: &egui::Context, session: &CanvasSession, bones: &[Bone]) -> Result<Self,String> {
        let mut sprites=Vec::new(); let mut size=Vec2::ZERO;
        // Layer list is top-to-bottom; draw bottom-to-top.
        for l in session.layers().into_iter().rev() {
            let Some(bone)=bones.iter().position(|b|b.layer==l.id) else {continue;};
            if !l.visible || l.locked {return Err(format!("Unavailable layer: {}",l.name));}
            let (w,h,rgba)=session.rig_preview_pixels(l.id).ok_or_else(||format!("Cannot read {}",l.name))?;
            size=vec2(w as f32,h as f32);
            let (mut x0,mut y0,mut x1,mut y1)=(w,h,0,0);
            for y in 0..h { for x in 0..w { if rgba[((y as usize*w as usize+x as usize)*4)+3]!=0 {x0=x0.min(x);y0=y0.min(y);x1=x1.max(x+1);y1=y1.max(y+1);} } }
            if x0>=x1 || y0>=y1 {return Err(format!("Empty layer: {}",l.name));}
            let mut crop=Vec::new();
            for y in y0..y1 {let start=(y as usize*w as usize+x0 as usize)*4;let end=start+(x1-x0) as usize*4;crop.extend_from_slice(&rgba[start..end]);}
            let image=egui::ColorImage::from_rgba_premultiplied([(x1-x0) as usize,(y1-y0) as usize],&crop);
            let texture=ctx.load_texture(format!("rig sprite {}",l.id),image,egui::TextureOptions::LINEAR);
            sprites.push(Sprite{bone,texture,min:pos2(x0 as f32,y0 as f32),max:pos2(x1 as f32,y1 as f32),opacity:l.opacity});
        }
        if sprites.len()!=bones.len() {return Err("A bound layer is missing".into());}
        let angles=vec![0.0;bones.len()];let mut keys=std::collections::BTreeMap::new();keys.insert(0,angles.clone());
        Ok(Self{bones:bones.to_vec(),sprites,size,keys,angles,frame:0,length:24,fps:12,playing:false,clock:0.0,start_frame:0,looping:true})
    }
    fn sample(&mut self) {
        let a=self.keys.range(..=self.frame).next_back();let b=self.keys.range(self.frame..).next();
        match (a,b) {
            (Some((fa,va)),Some((fb,vb))) if fa!=fb=>{let t=(self.frame-*fa) as f32/(*fb-*fa) as f32;self.angles=va.iter().zip(vb).map(|(a,b)|a+(b-a)*t).collect();},
            (Some((_,v)),_)|(_,Some((_,v)))=>self.angles=v.clone(),_=>self.angles.fill(0.0),
        }
    }
    fn map(&self, mut p:Pos2, mut i:usize)->Pos2 {
        loop {let b=&self.bones[i];p=rotated(p,b.joint,self.angles[i].to_radians());match b.parent {Some(parent) if parent<i=>i=parent,_=>break,}}
        p
    }
    fn show(&mut self,ctx:&egui::Context)->bool {
        let now=ctx.input(|i|i.time);
        if self.playing {
            let elapsed=((now-self.clock).max(0.0)*self.fps as f64) as u32;
            let next=self.start_frame.saturating_add(elapsed);
            if self.looping {self.frame=next%self.length;} else {self.frame=next.min(self.length-1);if next>=self.length-1 {self.playing=false;}}
            self.sample();ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        let mut open=true;
        egui::Window::new("Rig Animation — preview").open(&mut open).default_width(460.0).show(ctx,|ui|{
            ui.label("Preview only. Original artwork is untouched. Close to edit the rig; reopening captures a new rest pose.");
            ui.horizontal(|ui|{
                if ui.button(if self.playing {"Pause"}else{"Play"}).clicked(){self.playing=!self.playing;self.clock=now;self.start_frame=self.frame;self.sample();}
                ui.checkbox(&mut self.looping,"Loop");
                ui.add_enabled(!self.playing,egui::DragValue::new(&mut self.fps).range(1..=60).prefix("FPS "));
                let min_len=self.keys.keys().next_back().copied().unwrap_or(0)+1;
                ui.add_enabled(!self.playing,egui::DragValue::new(&mut self.length).range(min_len.max(2)..=600).prefix("Frames "));
            });
            self.frame=self.frame.min(self.length-1);
            if ui.add_enabled(!self.playing,egui::Slider::new(&mut self.frame,0..=self.length-1).text("Frame (0-based)")).changed(){self.sample();}
            ui.horizontal_wrapped(|ui|{ui.label("Keys:");let frames:Vec<u32>=self.keys.keys().copied().collect();for f in frames {if ui.add_enabled(!self.playing,egui::Button::new(f.to_string())).clicked(){self.frame=f;self.sample();}}});
            ui.add_enabled_ui(!self.playing,|ui|{
                ui.horizontal(|ui|{
                    if ui.button("Add / update keyframe").clicked(){self.keys.insert(self.frame,self.angles.clone());}
                    if ui.button("Delete keyframe").clicked(){self.keys.remove(&self.frame);self.sample();}
                    if ui.button("Reset pose").clicked(){self.angles.fill(0.0);}
                });
                egui::ScrollArea::vertical().id_salt("rig angles").max_height(150.0).show(ui,|ui|{
                    for (i,b) in self.bones.iter().enumerate(){ui.add(egui::Slider::new(&mut self.angles[i],-360.0..=360.0).text(&b.name));}
                });
            });
            let width=ui.available_width().max(100.0);let (r,_)=ui.allocate_exact_size(vec2(width,300.0),egui::Sense::hover());
            let painter=ui.painter().with_clip_rect(r);painter.rect_filled(r,0.0,Color32::from_gray(45));
            let scale=(r.width()/self.size.x).min(r.height()/self.size.y);let origin=r.center()-self.size*scale*0.5;
            let screen=|p:Pos2|origin+p.to_vec2()*scale;
            for s in &self.sprites {
                let mut mesh=egui::Mesh::with_texture(s.texture.id());let color=Color32::WHITE.gamma_multiply(s.opacity.clamp(0.0,1.0));
                for (p,uv) in [(s.min,pos2(0.0,0.0)),(pos2(s.max.x,s.min.y),pos2(1.0,0.0)),(s.max,pos2(1.0,1.0)),(pos2(s.min.x,s.max.y),pos2(0.0,1.0))] {
                    mesh.vertices.push(egui::epaint::Vertex{pos:screen(self.map(p,s.bone)),uv,color});
                }
                mesh.indices.extend_from_slice(&[0,1,2,0,2,3]);painter.add(egui::Shape::mesh(mesh));
            }
        });open
    }
}
