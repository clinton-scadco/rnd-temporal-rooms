// Use the machine designer's existing Plan renderer in the live draft bench.
import { state, box } from './doc.js';
import * as render from './render.js';
let cv,ctx,hooks,source,cat,sel,holding,level=0,face=0,pointer=null,live=null,drag=null,moved=false;
const view={ox:45,oy:45,scale:1,width:0,height:0,dpr:1};
export function init(canvas,callbacks){
 cv=canvas;ctx=cv.getContext('2d');hooks=callbacks;
 const position=e=>{const r=cv.getBoundingClientRect();return{x:(e.clientX-r.left-view.ox)/view.scale,y:(e.clientY-r.top-view.oy)/view.scale};};
 cv.addEventListener('mousedown',e=>{if(e.button===1||(e.button===0&&!holding)){drag={x:e.clientX,y:e.clientY,ox:view.ox,oy:view.oy};moved=false;}});
 window.addEventListener('mouseup',()=>{drag=null;});
 cv.addEventListener('click',e=>{if(moved){moved=false;return;}sync();const p=position(e),hit=render.hitUnit(render.layout(),p.x,p.y);if(holding)hooks.onGround(Math.round(p.x/render.TILE),Math.round(p.y/render.TILE));else if(hit)hooks.onPick(hit.u.name);});
 cv.addEventListener('mousemove',e=>{if(drag){if(Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>3)moved=true;if(moved){view.ox=drag.ox+e.clientX-drag.x;view.oy=drag.oy+e.clientY-drag.y;draw();return;}}sync();const p=position(e),hit=render.hitUnit(render.layout(),p.x,p.y);pointer={x:Math.round(p.x/render.TILE),y:Math.round(p.y/render.TILE)};hooks.onHover({hit:hit?{name:hit.u.name}:null,tile:{x:Math.round(p.x/render.TILE),y:Math.round(p.y/render.TILE)},at:[p.x/render.TILE*2,level*2,p.y/render.TILE*2]});});
 cv.addEventListener('contextmenu',e=>{e.preventDefault();hooks.onCancel();});
 cv.addEventListener('wheel',e=>{e.preventDefault();const p=position(e),r=cv.getBoundingClientRect();view.scale=Math.min(2.5,Math.max(.25,view.scale*Math.exp(-e.deltaY*.001)));view.ox=e.clientX-r.left-p.x*view.scale;view.oy=e.clientY-r.top-p.y*view.scale;draw();},{passive:false});
 window.addEventListener('resize',resize);
}
function sync(){if(!cat)return;state.cat={parts:Object.fromEntries(cat.parts.map(p=>[p.kind,p])),order:cat.parts.map(p=>p.kind),constants:cat.constants};state.design=source||{units:[],wires:[]};state.snapshot=live;state.faults=[];state.selected=sel?{what:'unit',name:sel}:null;}
export function show(design,catalogue,options={}){source=design;cat=catalogue;live=options.snapshot||null;sel=options.selected;holding=options.holding;level=options.level||0;face=options.face||0;sync();if(options.refit)fit();draw();}
export function resize(){if(!cv)return;const r=cv.getBoundingClientRect();view.width=r.width;view.height=r.height;view.dpr=Math.min(2,window.devicePixelRatio||1);cv.width=Math.max(1,Math.round(r.width*view.dpr));cv.height=Math.max(1,Math.round(r.height*view.dpr));draw();}
export function fit(){resize();if(!source?.units.length)return;sync();const bs=source.units.map(box),x0=Math.min(...bs.map(b=>b.x))*render.TILE,y0=Math.min(...bs.map(b=>b.y))*render.TILE,x1=Math.max(...bs.map(b=>b.x+b.w))*render.TILE,y1=Math.max(...bs.map(b=>b.y+b.h))*render.TILE;view.scale=Math.min(1.5,Math.max(.25,Math.min((view.width-110)/(x1-x0),(view.height-110)/(y1-y0))));view.ox=(view.width-(x1-x0)*view.scale)/2-x0*view.scale;view.oy=(view.height-(y1-y0)*view.scale)/2-y0*view.scale;draw();}
function draw(){if(!cv||!cat||!source)return;sync();const part=cat.parts.find(p=>p.kind===holding),rotated=face&1;
 const w=part?(rotated?part.h:part.w):0,h=part?(rotated?part.w:part.h):0;
 const valid=pointer&&pointer.x>=0&&pointer.y>=0&&!source.units.some(u=>{const b=box(u);return pointer.x<b.x+b.w&&pointer.x+w>b.x&&pointer.y<b.y+b.h&&pointer.y+h>b.y&&level<b.z+b.d&&level+(part?.storeys||1)>b.z;});
 render.draw(ctx,view,{scale:view.scale,font:'system-ui',renderTime:0,flowLabels:false,place:pointer?holding:null,placeAt:pointer,placeFace:face,placeOk:valid});}
