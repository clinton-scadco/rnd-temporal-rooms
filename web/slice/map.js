import { paintLand, paintBuildings, patchAt, ERAS } from './landscape.js';
let cv,ctx,frame,land,statics,projection;
let onPick=()=>{},onGate=()=>{};
export function init(canvas,opts={}){
  cv=canvas;ctx=cv.getContext('2d');onPick=opts.onPick||onPick;onGate=opts.onGate||onGate;
  cv.addEventListener('click',e=>{const p=position(e);if(!p)return;
    const g=land.geography.crossings.find(g=>Math.hypot(g.x-p.x,g.y-p.y)<4);if(g)return onGate(g.tag);
    const patch=patchAt(land.geography,p.x,p.y);if(patch)onPick(patch.tag);
  });
  cv.addEventListener('mousemove',e=>{const p=position(e);cv.style.cursor=p&&patchAt(land?.geography,p.x,p.y)?'pointer':'default';});
  window.addEventListener('resize',resize);
}
export function setStatics(s){statics=s;}
export function setLand(l){land=l;}
export function show(v){frame=v;draw();}
export function resize(){if(!cv)return;const r=cv.getBoundingClientRect(),d=window.devicePixelRatio||1;cv.width=Math.round(r.width*d);cv.height=Math.round(r.height*d);draw();}
function position(e){if(!projection)return null;const r=cv.getBoundingClientRect();return{x:(e.clientX-r.left-projection.x)/projection.s,y:(e.clientY-r.top-projection.y)/projection.s};}
function draw(){
  if(!ctx||!frame||!land?.geography)return;
  const r=cv.getBoundingClientRect(),d=window.devicePixelRatio||1;ctx.setTransform(d,0,0,d,0,0);ctx.clearRect(0,0,r.width,r.height);ctx.fillStyle='#131b1d';ctx.fillRect(0,0,r.width,r.height);
  const geo=land.geography,s=Math.min((r.width-48)/geo.width,(r.height-100)/geo.height),x=(r.width-geo.width*s)/2,y=(r.height-geo.height*s)/2;
  projection={x,y,s};const sx=v=>x+v*s,sy=v=>y+v*s;
  paintLand(ctx,land,sx,sy,s);paintBuildings(ctx,frame.regions,sx,sy,s);
  for(const route of frame.shipping?.routes||[]){
    const a=frame.regions.find(r=>r.tag===route.from),b=frame.regions.find(r=>r.tag===route.to);if(!a||!b)continue;
    const ax=a.offsetX+36,bx=b.offsetX+36;ctx.strokeStyle=ERAS[a.phase];ctx.globalAlpha=.65;ctx.lineWidth=2;
    ctx.beginPath();ctx.moveTo(sx(ax),sy(48));ctx.lineTo(sx(ax),sy(17));ctx.lineTo(sx(bx),sy(17));ctx.lineTo(sx(bx),sy(48));ctx.stroke();ctx.globalAlpha=1;
  }
  for(const p of geo.patches){
    const region=frame.regions.find(r=>r.tag===p.tag);if(!region)continue;const tx=sx(p.offsetX+36),ty=sy(64);
    ctx.fillStyle='rgba(14,21,23,.88)';ctx.fillRect(tx-94,ty-15,188,47);ctx.textAlign='center';ctx.fillStyle=ERAS[p.phase];ctx.font='600 12px system-ui';ctx.fillText(region.title,tx,ty);
    ctx.fillStyle='#b8c3bd';ctx.font='10px system-ui';ctx.fillText(`${region.machines} machines · ${region.gridMW} MW`,tx,ty+17);
  }
  for(const c of geo.crossings){
    const g=frame.shipping?.interfaces.find(g=>g.fracture===c.tag);ctx.fillStyle=g?.lit?'#c6b68b':'#676b63';ctx.fillRect(sx(c.x)-5,sy(c.y)-9,10,18);ctx.strokeStyle='#e7d8ad';ctx.lineWidth=1;ctx.strokeRect(sx(c.x)-5,sy(c.y)-9,10,18);
    ctx.textAlign='center';ctx.font='10px system-ui';ctx.fillStyle='#d9d5c1';ctx.fillText(`${c.tag} crossing`,sx(c.x),sy(c.y)-15);
  }
  ctx.textAlign='left';ctx.font='11px system-ui';ctx.fillStyle='#a6b4ae';ctx.fillText('One valley · neighbouring histories · select a region to build',24,26);
}
