import { view } from '../room/world.js';
import { paintLand, paintBuildings } from './landscape.js';
let cv,ctx,land,phase,regions=[],current=null,last='';
export function init(canvas){cv=canvas;ctx=cv.getContext('2d');window.addEventListener('resize',resize);
  const tick=()=>{const sig=`${view.ox}|${view.oy}|${view.scale}|${view.w}|${view.h}|${phase}`;if(sig!==last){last=sig;draw();}requestAnimationFrame(tick);};requestAnimationFrame(tick);
}
export function setLand(l){land=l;last='';}
export function setPhase(p){phase=p;last='';}
export function setWorld(r,tag){regions=r;current=tag;draw();}
export function resize(){if(!cv)return;const r=cv.getBoundingClientRect(),d=Math.min(2,window.devicePixelRatio||1);cv.width=Math.max(1,Math.round(r.width*d));cv.height=Math.max(1,Math.round(r.height*d));last='';draw();}
export function here(){
  if(!land||!phase)return[];
  return land.features.filter(f=>f.tag!=='rift').map(f=>({name:f.name,became:f.became,x:f.x,y:f.y,w:f.w,h:f.h,...f.faces.find(v=>v.phase===phase)})).filter(f=>f.what && f.what!=='open ground');
}
function draw(){
  if(!ctx||!cv||!land?.geography)return;const r=cv.getBoundingClientRect(),d=Math.min(2,window.devicePixelRatio||1);
  ctx.setTransform(d,0,0,d,0,0);ctx.clearRect(0,0,r.width,r.height);
  const s=7*view.scale,sx=x=>view.ox+x*s,sy=y=>view.oy+y*s;
  paintLand(ctx,land,sx,sy,s,current);paintBuildings(ctx,regions,sx,sy,s,current);
  for(const c of land.geography.crossings){ctx.fillStyle='#bcae8b';ctx.fillRect(sx(c.x)-s*.5,sy(c.y)-s*1.2,s,s*2.4);ctx.strokeStyle='#e2d5b3';ctx.lineWidth=1;ctx.strokeRect(sx(c.x)-s*.5,sy(c.y)-s*1.2,s,s*2.4);}
}
