//! Experimental desktop cutout rig. Joint coordinates are in artwork pixels.
use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};
use procraft_core::CanvasSession;

#[derive(Clone)]
struct Bone { layer: u64, name: String, parent: Option<usize>, joint: Pos2, tip: Pos2 }
struct Drag;
#[derive(Default)]
pub struct Rig { enabled: bool, pose: bool, bones: Vec<Bone>, selected: Option<usize>, bind_parent: Option<usize>, drag: Option<Drag>, message: String, animation: Option<Animation> }
fn rotated(p: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let d = p-origin; let (s,c)=angle.sin_cos(); origin+vec2(c*d.x-s*d.y,s*d.x+c*d.y)
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
                        }
                    });
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
                self.pose=false;
                ui.label("Place joints here. Pose safely in Rig Animation (original pixels preserved).");
                egui::ComboBox::from_id_salt("rig_bind_parent").selected_text(self.bind_parent.and_then(|i|self.bones.get(i)).map(|b|b.name.as_str()).unwrap_or("None (root)")).show_ui(ui,|ui|{
                    ui.selectable_value(&mut self.bind_parent,None,"None (root)");
                    for (i,b) in self.bones.iter().enumerate(){ui.selectable_value(&mut self.bind_parent,Some(i),format!("{}: {}",i+1,b.name));}
                });
                if ui.button("Bind current layer").clicked(){
                    if let Some(layer)=session.layers().into_iter().find(|l|l.current && l.visible && !l.locked && !l.text && l.plugin_kind.is_none() && matches!(l.kind,procraft_core::LayerType::Paint)) {
                        if self.bones.iter().any(|b|b.layer==layer.id){self.message="Layer already bound".into();}
                        else {let joint=canvas(rect.center());let tip=canvas(rect.center()+vec2(0.0,65.0));self.bones.push(Bone{layer:layer.id,name:layer.name,parent:self.bind_parent,joint,tip});self.selected=Some(self.bones.len()-1);self.message.clear();}
                    } else {self.message="Select an unlocked visible paint layer".into();}
                }
                if ui.button("Save skeleton").clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("skeleton.rigbones").save_file(){let mut data=String::from("RIGBONES1\n");for b in &self.bones{data+=&format!("{} {} {} {} {} {}\n",b.layer,b.parent.map(|p|p as i64).unwrap_or(-1),b.joint.x,b.joint.y,b.tip.x,b.tip.y);}self.message=match std::fs::write(path,data){Ok(())=>"Skeleton saved".into(),Err(e)=>e.to_string()};}}
                if ui.button("Load skeleton").clicked(){if let Some(path)=rfd::FileDialog::new().pick_file(){let result=(||->Result<Vec<Bone>,String>{let data=std::fs::read_to_string(path).map_err(|e|e.to_string())?;let mut lines=data.lines();if lines.next()!=Some("RIGBONES1"){return Err("Invalid skeleton".into());}let layers=session.layers();let mut bones:Vec<Bone>=Vec::new();for line in lines{let v:Vec<_>=line.split_whitespace().collect();if v.len()!=6{return Err("Invalid bone".into());}let layer=v[0].parse::<u64>().map_err(|e|e.to_string())?;let parent=v[1].parse::<i64>().map_err(|e|e.to_string())?;if parent < -1 || parent>=bones.len() as i64 || bones.iter().any(|b|b.layer==layer){return Err("Invalid hierarchy".into());}let l=layers.iter().find(|l|l.id==layer).ok_or("Layer missing; open original artwork")?;let p=v[2..].iter().map(|x|x.parse::<f32>()).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;if p.iter().any(|x|!x.is_finite()){return Err("Invalid coordinates".into());}bones.push(Bone{layer,name:l.name.clone(),parent:if parent<0{None}else{Some(parent as usize)},joint:pos2(p[0],p[1]),tip:pos2(p[2],p[3])});}Ok(bones)})();match result{Ok(b)=>{self.bones=b;self.selected=None;self.bind_parent=None;self.message="Skeleton loaded".into();},Err(e)=>self.message=e}}}
                if ui.button("Clear parent selection").clicked(){self.bind_parent=None;}
                for (i,b) in self.bones.iter().enumerate(){if ui.selectable_label(self.selected==Some(i),format!("{}: {} (parent {:?})",i+1,b.name,b.parent.map(|p|p+1))).clicked(){self.selected=Some(i);}}
                if let Some(i)=self.selected.filter(|&i|i<self.bones.len()) {
                    let mut parent=self.bones[i].parent;
                    ui.label(format!("Edit parent of {}",self.bones[i].name));
                    egui::ComboBox::from_id_salt("rig_edit_parent").selected_text(parent.and_then(|p|self.bones.get(p)).map(|b|b.name.as_str()).unwrap_or("None (root)")).show_ui(ui,|ui|{
                        ui.selectable_value(&mut parent,None,"None (root)");
                        for (p,b) in self.bones.iter().take(i).enumerate(){ui.selectable_value(&mut parent,Some(p),format!("{}: {}",p+1,b.name));}
                    });
                    if parent!=self.bones[i].parent {self.bones[i].parent=parent;self.message="Parent updated".into();}
                }
                if ui.button("Clear rig (keeps artwork)").clicked(){self.bones.clear();self.selected=None;self.bind_parent=None;}
            });
            ui.label(&self.message);
        });
        if !self.enabled { self.animation=None; }
        if let Some(a)=self.animation.as_mut() { if !a.show(ctx) { self.animation=None; } }
    }
}


// The animation owns immutable textures; preview never writes to the document.
struct Sprite { bone: usize, texture: egui::TextureHandle, min: Pos2, max: Pos2, opacity: f32, pixels: Vec<u8> }
struct Animation {
    expanded: bool,
    ik: bool, mesh_strength: f32, waveform: Vec<f32>, limits: Vec<[f32;2]>, continuous_audio: bool, clipboard: Option<Vec<f32>>, track: usize, graph: bool, skeleton: bool, curve: [f32; 2], spring: f32, damping: f32, poses: Vec<Vec<f32>>, audio: Option<std::path::PathBuf>, audio_player: Option<std::process::Child>, easing: usize, onion: bool, status: String, bones: Vec<Bone>, sprites: Vec<Sprite>, size: Vec2,
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
            sprites.push(Sprite{bone,texture,min:pos2(x0 as f32,y0 as f32),max:pos2(x1 as f32,y1 as f32),opacity:l.opacity,pixels:crop});
        }
        if sprites.len()!=bones.len() {return Err("A bound layer is missing".into());}
        let angles=vec![0.0;bones.len()*5];let mut keys=std::collections::BTreeMap::new();keys.insert(0,angles.clone());
        Ok(Self{expanded:false,ik:false,mesh_strength:0.0,waveform:Vec::new(),limits:vec![[-360.0,360.0];bones.len()],continuous_audio:true,clipboard:None,track:0,graph:true,skeleton:true,curve:[0.25,0.75],spring:0.0,damping:5.0,poses:Vec::new(),audio:None,audio_player:None,easing:0,onion:false,status:String::new(),bones:bones.to_vec(),sprites,size,keys,angles,frame:0,length:24,fps:12,playing:false,clock:0.0,start_frame:0,looping:true})
    }
    fn sample(&mut self) {
        let a=self.keys.range(..=self.frame).next_back();let b=self.keys.range(self.frame..).next();
        match (a,b) {
            (Some((fa,va)),Some((fb,vb))) if fa!=fb=>{let t=(self.frame-*fa) as f32/(*fb-*fa) as f32;let t=self.ease(t);self.angles=va.iter().zip(vb).map(|(a,b)|a+(b-a)*t).collect();},
            (Some((_,v)),_)|(_,Some((_,v)))=>self.angles=v.clone(),_=>self.angles.fill(0.0),
        }
    }
    fn ease(&self,t:f32)->f32 {
        match self.easing {8=>{let u=1.0-t;3.0*u*u*t*self.curve[0]+3.0*u*t*t*self.curve[1]+t*t*t},9=>{if t==0.0||t==1.0{t}else{1.0-(-self.damping*t).exp()*(t*18.0).cos()}},1=>t*t,2=>1.0-(1.0-t).powi(2),3=>t*t*(3.0-2.0*t),4=>(t*5.0).floor()/5.0,5=>{let u=t-1.0;1.0+2.70158*u*u*u+1.70158*u*u},6=>{if t==0.0 || t==1.0 {t} else {1.0-2.0_f32.powf(-10.0*t)*(t*20.0).cos()}},7=>{let (n,d)=(7.5625,2.75);if t<1.0/d {n*t*t} else if t<2.0/d {n*(t-1.5/d).powi(2)+0.75} else if t<2.5/d {n*(t-2.25/d).powi(2)+0.9375} else {n*(t-2.625/d).powi(2)+0.984375}},_=>t}
    }
    fn render_frame(&self)->Result<image::RgbaImage,String>{
        if self.mesh_strength>0.0{return Err("Mesh blending is preview-only; set strength to zero before export".into());}
        let (w,h)=(self.size.x as u32,self.size.y as u32);
        if w==0 || h==0 || u64::from(w)*u64::from(h)>16_777_216 {return Err("Export supports up to 16 million pixels per frame".into());}
        let mut out=image::RgbaImage::new(w,h);
        for s in &self.sprites {
            let a=self.map(s.min,s.bone);let u=self.map(pos2(s.max.x,s.min.y),s.bone)-a;let v=self.map(pos2(s.min.x,s.max.y),s.bone)-a;
            let det=u.x*v.y-u.y*v.x;if det.abs()<1e-8 {continue;}
            let corners=[a,a+u,a+v,a+u+v];
            let x0=corners.iter().map(|p|p.x).fold(f32::INFINITY,f32::min).floor().max(0.0) as u32;
            let y0=corners.iter().map(|p|p.y).fold(f32::INFINITY,f32::min).floor().max(0.0) as u32;
            let x1=corners.iter().map(|p|p.x).fold(f32::NEG_INFINITY,f32::max).ceil().max(0.0) as u32;
            let y1=corners.iter().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max).ceil().max(0.0) as u32;
            let sw=(s.max.x-s.min.x) as usize;let sh=(s.max.y-s.min.y) as usize;
            for y in y0..y1.min(h){for x in x0..x1.min(w){let d=pos2(x as f32+0.5,y as f32+0.5)-a;let tx=(d.x*v.y-d.y*v.x)/det;let ty=(u.x*d.y-u.y*d.x)/det;
                if !(0.0..1.0).contains(&tx)||!(0.0..1.0).contains(&ty){continue;}
                let i=((ty*sh as f32) as usize*sw+(tx*sw as f32) as usize)*4;let src=&s.pixels[i..i+4];let sa=src[3] as f32/255.0*s.opacity.clamp(0.0,1.0);if sa<=0.0{continue;}
                let dst=out.get_pixel_mut(x,y);let da=dst[3] as f32/255.0;let oa=sa+da*(1.0-sa);
                for c in 0..3 {dst[c]=((src[c] as f32*s.opacity.clamp(0.0,1.0)+dst[c] as f32*da*(1.0-sa))/oa).round().clamp(0.0,255.0) as u8;}dst[3]=(oa*255.0).round() as u8;
            }}
        } Ok(out)
    }
    fn export(&mut self,gif:bool)->Result<(),String>{
        let path=if gif {rfd::FileDialog::new().add_filter("GIF",&["gif"]).set_file_name("animation.gif").save_file()} else {rfd::FileDialog::new().pick_folder()};
        let Some(path)=path else{return Ok(());};
        let old=self.frame;let pose=self.angles.clone();
        let result=(||->Result<(),String>{
            if gif {let file=std::fs::File::create(&path).map_err(|e|e.to_string())?;let mut enc=image::codecs::gif::GifEncoder::new_with_speed(file,10);if self.looping {enc.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e|e.to_string())?;}
                for f in 0..self.length {self.frame=f;self.sample();let img=self.render_frame()?;enc.encode_frame(image::Frame::from_parts(img,0,0,image::Delay::from_numer_denom_ms(1000,self.fps))).map_err(|e|e.to_string())?;}
            } else {for f in 0..self.length {self.frame=f;self.sample();self.render_frame()?.save(path.join(format!("rig-frame-{f:04}.png"))).map_err(|e|e.to_string())?;}}
            Ok(())})();self.frame=old;self.angles=pose;result
    }
    fn map(&self, mut p:Pos2, mut i:usize)->Pos2 {
        loop {let b=&self.bones[i];let k=i*5; let d=p-b.joint; p=b.joint+vec2(d.x*(1.0+self.angles[k+3]).max(0.05),d.y*(1.0+self.angles[k+4]).max(0.05)); let wobble=self.spring*(self.frame as f32/self.fps as f32*12.0-i as f32*0.5).sin(); p=rotated(p,b.joint,(self.angles[k]+wobble).to_radians())+vec2(self.angles[k+1],self.angles[k+2]);match b.parent {Some(parent) if parent<i=>i=parent,_=>break,}}
        p
    }
    // CCD in document space; adjusts ancestors and respects local angle limits.
    fn solve_ik(&mut self, target: Pos2) {
        let end=self.track; let mut chain=vec![end]; let mut j=end;
        while let Some(p)=self.bones[j].parent {if p>=j {break;} chain.push(p);j=p;}
        for _ in 0..16 {for &i in &chain {
            let pivot=self.map(self.bones[i].joint,i);let a=self.map(self.bones[end].tip,end)-pivot;let b=target-pivot;
            if a.length_sq()<0.0001 || b.length_sq()<0.0001 {continue;}
            let delta=(a.x*b.y-a.y*b.x).atan2(a.dot(b)).to_degrees();
            self.angles[i*5]=(self.angles[i*5]+delta).clamp(self.limits[i][0],self.limits[i][1]);
        } if (self.map(self.bones[end].tip,end)-target).length_sq()<0.25 {break;} }
    }
    fn read_waveform(&mut self) {
        self.waveform.clear(); let Some(path)=&self.audio else{return;};
        let result=std::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(path).args(["-t","600","-ac","1","-ar","1000","-f","s16le","-"]).output();
        match result {Ok(o) if o.status.success()=>{self.waveform=o.stdout.chunks_exact(2).map(|b|i16::from_le_bytes([b[0],b[1]]) as f32/32768.0).collect();},_=>self.status="Waveform requires FFmpeg".into()}
    }
    fn deform(&self,p:Pos2,i:usize)->Pos2 {
        let a=self.map(p,i);let Some(parent)=self.bones[i].parent else{return a;};
        let bone=&self.bones[i];let radius=(bone.tip-bone.joint).length().max(1.0)*0.65;
        let t=(1.0-(p-bone.joint).length()/radius).clamp(0.0,1.0);let w=self.mesh_strength*t*t*(3.0-2.0*t);
        a+(self.map(p,parent)-a)*w
    }
    fn sprite_mesh(&self,s:&Sprite,screen:impl Fn(Pos2)->Pos2,alpha:f32)->egui::Mesh {
        let mut mesh=egui::Mesh::with_texture(s.texture.id());let n=if self.mesh_strength>0.0 {16u32}else{1};
        for y in 0..=n {for x in 0..=n {let u=x as f32/n as f32;let v=y as f32/n as f32;let p=pos2(s.min.x+(s.max.x-s.min.x)*u,s.min.y+(s.max.y-s.min.y)*v);mesh.vertices.push(egui::epaint::Vertex{pos:screen(self.deform(p,s.bone)),uv:pos2(u,v),color:Color32::WHITE.gamma_multiply(alpha*s.opacity.clamp(0.0,1.0))});}}
        for y in 0..n {for x in 0..n {let a=y*(n+1)+x;let b=a+1;let c=a+n+1;mesh.indices.extend_from_slice(&[a,b,c,b,c+1,c]);}}mesh
    }
    fn preset(&mut self,shake:bool){let base=self.angles.clone();for j in 0..=4 {let f=(self.length-1)*j/4;let mut p=base.clone();if j%2==1 {for i in 0..self.bones.len(){if shake {p[i*5]+=if j==1 {12.0}else{-12.0};}else if self.bones[i].parent.is_none(){p[i*5+2]-=20.0;p[i*5+3]-=0.1;p[i*5+4]+=0.1;}}}self.keys.insert(f,p);}self.sample();}
    fn stop_audio(&mut self){if let Some(mut c)=self.audio_player.take(){let _=c.kill();let _=c.wait();}}
    fn start_audio(&mut self){if let Some(p)=&self.audio {match std::process::Command::new("ffplay").args(["-nodisp","-autoexit","-loglevel","error","-ss",&format!("{}",self.frame as f64/self.fps as f64)]).arg(p).spawn(){Ok(c)=>self.audio_player=Some(c),Err(e)=>self.status=format!("Audio: {e}")}}}
    fn save_motion(&self)->Result<String,String>{let Some(p)=rfd::FileDialog::new().set_file_name("motion.rig").save_file() else{return Ok("Cancelled".into());};let mut t=format!("RIG3 {} {} {} {} {} {} {} {}\n",self.bones.len(),self.length,self.fps,self.easing,self.curve[0],self.curve[1],self.spring,self.damping);for (f,v) in &self.keys {t+=&format!("{} {}\n",f,v.iter().map(|x|x.to_string()).collect::<Vec<_>>().join(" "));}std::fs::write(p,t).map_err(|e|e.to_string())?;Ok("Motion saved".into())}
    fn load_motion(&mut self)->Result<String,String>{let Some(p)=rfd::FileDialog::new().pick_file() else{return Ok("Cancelled".into());};let t=std::fs::read_to_string(p).map_err(|e|e.to_string())?;let mut lines=t.lines();let h:Vec<_>=lines.next().unwrap_or("").split_whitespace().collect();if h.len()!=9||h[0]!="RIG3"||h[1].parse::<usize>().ok()!=Some(self.bones.len()){return Err("Motion requires the same bone count and order".into());}let len=h[2].parse::<u32>().map_err(|e|e.to_string())?;let fps=h[3].parse::<u32>().map_err(|e|e.to_string())?;let ease=h[4].parse::<usize>().map_err(|e|e.to_string())?;if !(2..=600).contains(&len)||!(1..=60).contains(&fps)||ease>9{return Err("Invalid motion header".into());}let settings=h[5..].iter().map(|x|x.parse::<f32>()).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;if settings.iter().any(|x|!x.is_finite()){return Err("Invalid settings".into());}let mut keys=std::collections::BTreeMap::new();for line in lines {let mut w=line.split_whitespace();let f=w.next().unwrap_or("").parse::<u32>().map_err(|e|e.to_string())?;let v=w.map(|x|x.parse::<f32>()).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;if f>=len||v.len()!=self.bones.len()*5||v.iter().any(|x|!x.is_finite()){return Err("Invalid keyframe".into());}keys.insert(f,v);}self.stop_audio();self.playing=false;self.length=len;self.fps=fps;self.easing=ease;self.curve=[settings[0],settings[1]];self.spring=settings[2];self.damping=settings[3];self.keys=keys;self.frame=0;self.sample();Ok("Motion loaded".into())}
    fn video_export(&mut self,mp4:bool)->Result<String,String>{self.playing=false;self.stop_audio();let ext=if mp4{"mp4"}else{"webp"};let Some(path)=rfd::FileDialog::new().add_filter(ext,&[ext]).set_file_name(format!("animation.{ext}")).save_file() else{return Ok("Cancelled".into());};let dir=std::env::temp_dir().join(format!("rigcraft-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos()));std::fs::create_dir(&dir).map_err(|e|e.to_string())?;let old=self.frame;let pose=self.angles.clone();let result=(||->Result<String,String>{for f in 0..self.length{self.frame=f;self.sample();self.render_frame()?.save(dir.join(format!("frame-{f:04}.png"))).map_err(|e|e.to_string())?;}let mut cmd=std::process::Command::new("ffmpeg");cmd.args(["-y","-framerate",&self.fps.to_string(),"-i"]).arg(dir.join("frame-%04d.png"));if mp4 {if let Some(a)=&self.audio{cmd.arg("-i").arg(a).args(["-c:a","aac"]);}cmd.args(["-c:v","libx264","-vf","pad=ceil(iw/2)*2:ceil(ih/2)*2","-pix_fmt","yuv420p"]);}else{cmd.args(["-c:v","libwebp_anim","-loop",if self.looping{"0"}else{"1"}]);}let out=cmd.args(["-t",&format!("{}",self.length as f64/self.fps as f64)]).arg(path).output().map_err(|e|format!("FFmpeg required: {e}"))?;if !out.status.success(){return Err(String::from_utf8_lossy(&out.stderr).into_owned());}Ok("Export complete".into())})();self.frame=old;self.angles=pose;let _=std::fs::remove_dir_all(dir);result}
    fn show(&mut self,ctx:&egui::Context)->bool {
        let now=ctx.input(|i|i.time);
        if self.playing {
            let elapsed=((now-self.clock).max(0.0)*self.fps as f64) as u32;
            let next=self.start_frame.saturating_add(elapsed);
            if self.looping {let f=next%self.length;if f<self.frame && !self.continuous_audio {self.stop_audio();self.frame=f;self.start_audio();}self.frame=f;} else {self.frame=next.min(self.length-1);if next>=self.length {self.playing=false;self.stop_audio();}}
            self.sample();ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        let mut open=true;
        let screen = ctx.content_rect();
        let usable = vec2((screen.width()-32.0).max(160.0), (screen.height()-64.0).max(120.0));
        let expanded = self.expanded;
        let mut window = egui::Window::new("Rig Animation — preview")
            .open(&mut open).resizable(!expanded).collapsible(false)
            .default_width(760.0_f32.min(usable.x)).default_height(650.0_f32.min(usable.y))
            .min_size(vec2(160.0,120.0)).max_size(usable);
        if expanded { window = window.fixed_pos(screen.min + vec2(12.0,12.0)).fixed_size(usable); }
        window.show(ctx,|ui|{
            ui.horizontal_wrapped(|ui| {
                if ui.button(if expanded { "Restore window" } else { "Maximize window" }).clicked() { self.expanded = !expanded; }
                ui.label("Drag window edges to resize");
            });
            let workspace_height = ui.available_height().min(usable.y).max(80.0);
            let preview_height = (workspace_height * 0.38).clamp(40.0,360.0);

            let width=ui.available_width().max(100.0);let (r,preview_response)=ui.allocate_exact_size(vec2(width,preview_height),egui::Sense::click_and_drag());
            let painter=ui.painter().with_clip_rect(r);painter.rect_filled(r,0.0,Color32::from_gray(45));
            let scale=(r.width()/self.size.x).min(r.height()/self.size.y);let origin=r.center()-self.size*scale*0.5;
            let screen=|p:Pos2|origin+p.to_vec2()*scale;
            if !self.playing && preview_response.dragged(){if let Some(p)=preview_response.interact_pointer_pos(){
                let target=pos2((p.x-origin.x)/scale,(p.y-origin.y)/scale);
                if self.ik {self.solve_ik(target);} else if let Some(b)=self.bones.get(self.track){
                    let a=self.map(b.joint,self.track);let z=self.map(b.tip,self.track);let u=z-a;let v=target-a;
                    if u.length_sq()>0.01 && v.length_sq()>0.01 {let delta=v.y.atan2(v.x)-u.y.atan2(u.x);let k=self.track*5;self.angles[k]=(self.angles[k]+delta.sin().atan2(delta.cos()).to_degrees()).clamp(-360.0,360.0);}
                }
            }}
            if self.onion && self.frame>0 {let old=self.frame;let pose=self.angles.clone();self.frame-=1;self.sample();for s in &self.sprites {painter.add(egui::Shape::mesh(self.sprite_mesh(s,&screen,0.25)));}self.frame=old;self.angles=pose;}
            for s in &self.sprites {painter.add(egui::Shape::mesh(self.sprite_mesh(s,&screen,1.0)));}
            if self.skeleton {for (i,b) in self.bones.iter().enumerate(){let a=screen(self.map(b.joint,i));let z=screen(self.map(b.tip,i));let c=if i==self.track{Color32::YELLOW}else{Color32::LIGHT_GREEN};painter.line_segment([a,z],egui::Stroke::new(2.0,c));painter.circle_filled(a,3.0,c);painter.circle_stroke(z,4.0,egui::Stroke::new(1.0,c));}}
            ui.separator();
            egui::ScrollArea::vertical().id_salt("rig_workspace_controls").max_height((workspace_height-preview_height-24.0).max(40.0)).auto_shrink([false,false]).show(ui, |ui| {
            ui.label("RigCraft v0.4.1 — original artwork is untouched. Export includes bound layers only.");
            ui.horizontal_wrapped(|ui|{if ui.add_enabled(!self.playing,egui::Button::new("Export GIF")).clicked(){self.status=match self.export(true){Ok(())=>"Export finished or cancelled".into(),Err(e)=>e};}if ui.add_enabled(!self.playing,egui::Button::new("Export PNG sequence")).clicked(){self.status=match self.export(false){Ok(())=>"Export finished or cancelled".into(),Err(e)=>e};}});
            ui.label(&self.status);
            let names=["Linear","Ease in","Ease out","Smooth","Stepped","Overshoot","Elastic","Bounce","Custom curve","Spring"];
            let before=self.easing;egui::ComboBox::from_label("Easing").selected_text(names[self.easing]).show_ui(ui,|ui|{for (i,n) in names.iter().enumerate(){ui.selectable_value(&mut self.easing,i,*n);}});if before!=self.easing {self.sample();}
            if self.easing==8 {ui.add(egui::Slider::new(&mut self.curve[0],-1.0..=2.0).text("Curve control Y1"));ui.add(egui::Slider::new(&mut self.curve[1],-1.0..=2.0).text("Curve control Y2"));}
            ui.add(egui::Slider::new(&mut self.spring,0.0..=45.0).text("Procedural wobble"));ui.add(egui::Slider::new(&mut self.damping,1.0..=12.0).text("Spring damping"));
            ui.horizontal_wrapped(|ui|{if ui.button("Save motion").clicked(){self.status=self.save_motion().unwrap_or_else(|e|e);}if ui.button("Load motion").clicked(){self.status=self.load_motion().unwrap_or_else(|e|e);}if ui.button("Import MP3 / WAV / OGG").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Audio",&["mp3","wav","ogg"]).pick_file(){self.stop_audio();self.playing=false;self.audio=Some(p);self.read_waveform();}}if ui.button("Export WebP").clicked(){self.status=self.video_export(false).unwrap_or_else(|e|e);}if ui.button("Export MP4").clicked(){self.status=self.video_export(true).unwrap_or_else(|e|e);}});
            if let Some(p)=&self.audio {ui.label(format!("Audio: {} (FFplay required)",p.display()));}
            ui.checkbox(&mut self.onion,"Onion skin (previous frame)");
            ui.checkbox(&mut self.graph,"Easing graph");
            if self.graph {
                let (r,_)=ui.allocate_exact_size(vec2(ui.available_width().max(100.0),90.0),egui::Sense::hover());
                let p=ui.painter().with_clip_rect(r);p.rect_filled(r,0.0,Color32::from_gray(35));
                let point=|t:f32,y:f32|pos2(r.left()+t*r.width(),r.bottom()-(y+0.5)/2.0*r.height());
                p.line_segment([point(0.0,0.0),point(1.0,1.0)],egui::Stroke::new(1.0,Color32::GRAY));
                for i in 0..64 {let a=i as f32/64.0;let b=(i+1) as f32/64.0;p.line_segment([point(a,self.ease(a)),point(b,self.ease(b))],egui::Stroke::new(2.0,Color32::LIGHT_GREEN));}
            }
            ui.checkbox(&mut self.skeleton,"Preview skeleton");
            ui.checkbox(&mut self.ik,"IK: drag selected bone tip in preview");
            ui.label("IK off: drag preview to rotate the selected Bone track safely.");
            ui.add(egui::Slider::new(&mut self.mesh_strength,0.0..=1.0).text("Joint mesh blending (preview only)"));
            if !self.waveform.is_empty(){let (r,_)=ui.allocate_exact_size(vec2(ui.available_width().max(100.0),55.0),egui::Sense::hover());let p=ui.painter().with_clip_rect(r);p.rect_filled(r,0.0,Color32::from_gray(35));let count=((self.length as f32/self.fps as f32)*1000.0) as usize;for x in 0..256 {let a=x*count/256;let b=((x+1)*count/256).min(self.waveform.len());if a>=b {continue;}let peak=self.waveform[a..b].iter().fold(0.0_f32,|m,v|m.max(v.abs()));let px=r.left()+x as f32/256.0*r.width();p.line_segment([pos2(px,r.center().y-peak*25.0),pos2(px,r.center().y+peak*25.0)],egui::Stroke::new(1.0,Color32::LIGHT_GREEN));}}

            ui.horizontal_wrapped(|ui|{
                if ui.button(if self.playing {"Pause"}else{"Play"}).clicked(){self.playing=!self.playing;self.clock=now;self.start_frame=self.frame;self.sample();self.stop_audio();if self.playing {self.start_audio();}}
                ui.checkbox(&mut self.looping,"Loop");
                ui.checkbox(&mut self.continuous_audio,"Continuous audio");
                ui.add_enabled(!self.playing,egui::DragValue::new(&mut self.fps).range(1..=60).prefix("FPS "));
                let min_len=self.keys.keys().next_back().copied().unwrap_or(0)+1;
                ui.add_enabled(!self.playing,egui::DragValue::new(&mut self.length).range(min_len.max(2)..=600).prefix("Frames "));
            });
            self.frame=self.frame.min(self.length-1);
            if ui.add_enabled(!self.playing,egui::Slider::new(&mut self.frame,0..=self.length-1).text("Frame (0-based)")).changed(){self.sample();}
            ui.horizontal_wrapped(|ui|{ui.label("Keys:");let frames:Vec<u32>=self.keys.keys().copied().collect();for f in frames {if ui.add_enabled(!self.playing,egui::Button::new(f.to_string())).clicked(){self.frame=f;self.sample();}}});
            ui.add_enabled_ui(!self.playing,|ui|{
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Copy pose").clicked(){self.clipboard=Some(self.angles.clone());}
                    if ui.button("Paste pose").clicked(){if let Some(p)=&self.clipboard{self.angles=p.clone();}}
                    if ui.button("Repeat clip x2").clicked(){let n=self.length;if n<=300 {let saved=self.frame;self.frame=n-1;self.sample();self.keys.entry(n-1).or_insert_with(||self.angles.clone());self.frame=saved;let old=self.keys.clone();for (f,p) in old{self.keys.insert(f+n,p);}self.length=n*2;self.sample();}else{self.status="Repeat would exceed 600 frames".into();}}
                    if ui.button("Headbop preset").clicked(){let base=self.angles.clone();for j in 0..=4 {let f=(self.length-1)*j/4;let mut p=base.clone();if j%2==1 {for (i,b) in self.bones.iter().enumerate(){if b.name.to_lowercase().contains("head"){p[i*5+2]-=3.0;}}}self.keys.insert(f,p);}self.sample();}
                });
                egui::ComboBox::from_label("Bone track").selected_text(&self.bones[self.track].name).show_ui(ui,|ui|{for (i,b) in self.bones.iter().enumerate(){ui.selectable_value(&mut self.track,i,&b.name);}});
                let (r,response)=ui.allocate_exact_size(vec2(ui.available_width().max(100.0),38.0),egui::Sense::click());
                let painter=ui.painter().with_clip_rect(r);painter.rect_filled(r,0.0,Color32::from_gray(35));
                let x=|f:u32|r.left()+f as f32/(self.length-1) as f32*r.width();
                for f in self.keys.keys(){painter.circle_filled(pos2(x(*f),r.center().y),4.0,Color32::LIGHT_GREEN);}
                painter.line_segment([pos2(x(self.frame),r.top()),pos2(x(self.frame),r.bottom())],egui::Stroke::new(2.0,Color32::YELLOW));
                if response.clicked(){if let Some(p)=response.interact_pointer_pos(){self.frame=(((p.x-r.left())/r.width()).clamp(0.0,1.0)*(self.length-1) as f32).round() as u32;self.sample();}}
                ui.horizontal(|ui|{let limit=&mut self.limits[self.track];ui.add(egui::DragValue::new(&mut limit[0]).range(-360.0..=360.0).prefix("IK min "));ui.add(egui::DragValue::new(&mut limit[1]).range(-360.0..=360.0).prefix("IK max "));if limit[0]>limit[1]{limit[1]=limit[0];}});
                ui.horizontal_wrapped(|ui|{if ui.button("Key selected bone").clicked(){let pose=self.angles.clone();let k=self.track*5;self.sample();self.angles[k..k+5].copy_from_slice(&pose[k..k+5]);self.keys.insert(self.frame,self.angles.clone());}if ui.button("Move key left").clicked() && self.frame>0 && !self.keys.contains_key(&(self.frame-1)){if let Some(p)=self.keys.remove(&self.frame){self.frame-=1;self.keys.insert(self.frame,p);self.sample();}}if ui.button("Move key right").clicked() && self.frame+1<self.length && !self.keys.contains_key(&(self.frame+1)){if let Some(p)=self.keys.remove(&self.frame){self.frame+=1;self.keys.insert(self.frame,p);self.sample();}}});
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Add / update keyframe").clicked(){self.keys.insert(self.frame,self.angles.clone());}
                    if ui.button("Delete keyframe").clicked(){self.keys.remove(&self.frame);self.sample();}
                    if ui.button("Reset pose").clicked(){self.angles.fill(0.0);}
                });
                ui.group(|ui|{
                    for (i,b) in self.bones.iter().enumerate(){let k=i*5;ui.collapsing(&b.name,|ui|{ui.add(egui::Slider::new(&mut self.angles[k],-360.0..=360.0).text("Rotation"));for (j,label) in [(1,"X"),(2,"Y")] {ui.add(egui::DragValue::new(&mut self.angles[k+j]).prefix(label));}for (j,label) in [(3,"Scale X offset"),(4,"Scale Y offset")] {ui.add(egui::Slider::new(&mut self.angles[k+j],-0.95..=2.0).text(label));}if ui.button("Squash").clicked(){self.angles[k+3]=0.25;self.angles[k+4]=-0.2;}if ui.button("Stretch").clicked(){self.angles[k+3]=-0.2;self.angles[k+4]=0.25;}});}
                    ui.horizontal(|ui|{if ui.button("Store pose").clicked(){self.poses.push(self.angles.clone());}for (i,p) in self.poses.iter().enumerate(){if ui.button(format!("Pose {}",i+1)).clicked(){self.angles=p.clone();}}});
                    ui.horizontal(|ui|{if ui.button("Bounce preset").clicked(){self.preset(false);}if ui.button("Shake preset").clicked(){self.preset(true);}});
                });
            });
            });
        });if !open {self.stop_audio();}open
    }
}

impl Drop for Animation {fn drop(&mut self){self.stop_audio();}}
