// One local disturbance in the ordinary region view. Effects are interpolated
// from authoritative ticks; no browser state can damage a factory.
import { view } from '../room/world.js';
let cv,ctx,state,regions,region,received=0,frameTick=0;
export function init(canvas){cv=canvas;ctx=cv.getContext('2d');const tick=()=>{draw();requestAnimationFrame(tick);};requestAnimationFrame(tick);}
export function show(v,current,acts){
 state=v.disturbance;regions=v.regions;region=current;received=performance.now();frameTick=v.tick;
 const i=state?.incident,alert=document.getElementById('incidentalert');
 if(alert){alert.hidden=!i;if(i){alert.textContent=i.damaged?'Ore feed damaged':`Ore crossing · ${Math.ceil(i.secondsLeft)}s`;alert.onclick=()=>acts.focus(i.region,i.target);}}
 const box=document.getElementById('disturbancecard');if(!box)return;box.hidden=!i;if(!i)return;
 const target=regions.find(r=>r.tag===i.region)?.world.installs.find(b=>b.id===i.target);
 const expanded=box.querySelector('details')?.open;
 box.innerHTML=`<b>${i.damaged?'Ore feed damaged':'Deep crossing under strain'}</b><p>Ore imports threaten ${target?.name||'one feed connection'}. ${i.damaged?'Repair the feed to resume its supply.':`${Math.ceil(i.secondsLeft)}s before impact. Closing the ore route settles the crossing.`}</p><div class="actions"><button id="incidentfocus">Inspect feed</button><button id="incidentaction">${i.damaged?'Repair feed':'Close ore route'}</button></div><details><summary>Simulation</summary><p>1 local domain · ${state.cohorts} compact incident · connection damage enters the room command log. Other regions continue running.</p></details>`;
 if(expanded)box.querySelector('details').open=true;
 document.getElementById('incidentfocus').onclick=()=>acts.focus(i.region,i.target);
 document.getElementById('incidentaction').onclick=()=>i.damaged?acts.repair():acts.closeOre();
}
function draw(){
 if(!cv)return;const r=cv.getBoundingClientRect(),d=Math.min(2,window.devicePixelRatio||1);
 if(cv.width!==Math.round(r.width*d)||cv.height!==Math.round(r.height*d)){cv.width=Math.round(r.width*d);cv.height=Math.round(r.height*d);}
 ctx.setTransform(d,0,0,d,0,0);ctx.clearRect(0,0,r.width,r.height);
 const i=state?.incident;if(!i||i.region!==region)return;
 const s=7*view.scale,sx=x=>view.ox+x*s,sy=y=>view.oy+y*s;
 const tick=frameTick+(performance.now()-received)*.06,k=Math.min(1,Math.max(0,(tick-i.start)/(i.impact-i.start)));
 const target=regions?.find(r=>r.tag===i.region)?.world.installs.find(b=>b.id===i.target),w=target?.w||7,h=target?.h||6;
 const ax=sx(i.from[0]),ay=sy(i.from[1]),bx=sx(i.to[0]+w/2),by=sy(i.to[1]+h/2);
 ctx.strokeStyle=i.damaged?'#d48b72':'#d5b474';ctx.lineWidth=2;ctx.setLineDash([4,6]);
 ctx.beginPath();ctx.moveTo(ax,ay);ctx.lineTo(bx,ay);ctx.lineTo(bx,by);ctx.stroke();ctx.setLineDash([]);
 if(!i.damaged){
  const len=Math.abs(bx-ax)+Math.abs(by-ay),dist=len*k,first=Math.abs(bx-ax);
  const x=dist<=first?ax+Math.sign(bx-ax)*dist:bx,y=dist<=first?ay:ay+Math.sign(by-ay)*(dist-first);
  ctx.fillStyle='#e1c384';ctx.beginPath();ctx.arc(x,y,4,0,Math.PI*2);ctx.fill();
 }
 ctx.strokeStyle=i.damaged?'#df896f':'#ddbd83';ctx.lineWidth=2;
 ctx.strokeRect(sx(i.to[0])-4,sy(i.to[1])-4,w*s+8,h*s+8);
}
