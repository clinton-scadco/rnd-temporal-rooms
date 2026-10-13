// The overview and region use the same single, spatially fractured landscape.
export const ERAS = { '1890': '#c59b62', '2037': '#7ca4a1', '2070': '#9ca9bc' };
export function path(ctx, points, sx, sy) {
  ctx.beginPath(); points.forEach(([x,y],i)=>i?ctx.lineTo(sx(x),sy(y)):ctx.moveTo(sx(x),sy(y))); ctx.closePath();
}
export function paintLand(ctx,land,sx,sy,scale,selected) {
  const geo=land.geography;if(!geo)return;
  for(const p of geo.patches){
    ctx.save();path(ctx,p.polygon,sx,sy);ctx.clip();
    ctx.fillStyle=p.phase==='1890'?'#383c30':p.phase==='2037'?'#394140':'#414749';
    ctx.fillRect(sx(0),sy(0),geo.width*scale,geo.height*scale);
    ctx.fillStyle='rgba(224,216,180,.035)';
    for(let y=3;y<geo.height;y+=5)for(let x=1;x<geo.width;x+=7)ctx.fillRect(sx(x),sy(y),scale*1.4,scale*.45);
    for(const f of land.features){
      const face=f.faces.find(v=>v.phase===p.phase);if(!face||face.what==='open ground')continue;
      const x=f.x,w=f.w;
      const px=sx(x),py=sy(f.y),pw=w*scale,ph=f.h*scale;
      ctx.globalAlpha=face.taken?.8:face.yields?.6:.55;ctx.fillStyle=face.colour;ctx.fillRect(px,py,pw,ph);ctx.globalAlpha=1;
      if(face.layer==='roads'){
        ctx.strokeStyle=p.phase==='1890'?'#c1b28b':'#c2c2b1';ctx.lineWidth=Math.max(.7,scale*.12);
        ctx.setLineDash(p.phase==='1890'?[]:[scale*1.2,scale*1.8]);ctx.beginPath();ctx.moveTo(px,py+ph/2);ctx.lineTo(px+pw,py+ph/2);ctx.stroke();ctx.setLineDash([]);
      }else if(f.tag==='millrace'){
        ctx.strokeStyle='rgba(158,203,211,.22)';ctx.lineWidth=1;
        for(let k=1;k<4;k++){ctx.beginPath();ctx.moveTo(px,py+ph*k/4);ctx.lineTo(px+pw,py+ph*k/4);ctx.stroke();}
      }else if(face.layer==='vegetation'){
        ctx.fillStyle=p.phase==='1890'?'#516c46':'#778160';
        for(let ty=1;ty<f.h;ty+=2)for(let tx=1;tx<f.w;tx+=2){ctx.beginPath();ctx.arc(sx(x+tx),sy(f.y+ty),Math.max(.8,scale*.65),0,Math.PI*2);ctx.fill();}
      }else if(face.taken){
        ctx.strokeStyle='rgba(13,20,21,.55)';ctx.lineWidth=Math.max(.7,scale*.2);
        for(let k=2;k<w;k+=3){ctx.beginPath();ctx.moveTo(sx(x+k),py);ctx.lineTo(sx(x+k),py+ph);ctx.stroke();}ctx.strokeRect(px,py,pw,ph);
      }
    }
    if(selected&&selected!==p.tag){ctx.fillStyle='rgba(12,17,19,.22)';ctx.fillRect(sx(0),sy(0),geo.width*scale,geo.height*scale);}
    ctx.restore();
  }
  for(const p of geo.patches.slice(1)){
    ctx.strokeStyle='#b8a68c';ctx.lineWidth=Math.max(1.2,scale*.25);ctx.setLineDash([4,5]);ctx.beginPath();
    p.polygon.slice(0,p.polygon.length/2).forEach(([x,y],i)=>i?ctx.lineTo(sx(x),sy(y)):ctx.moveTo(sx(x),sy(y)));ctx.stroke();ctx.setLineDash([]);
  }
}
export function patchAt(geo,x,y){
  if(!geo||y<0||y>=geo.height||x<0||x>=geo.width)return null;
  return geo.patches.find(p=>{
    let inside=false;
    for(let i=0,j=p.polygon.length-1;i<p.polygon.length;j=i++){
      const [ax,ay]=p.polygon[i],[bx,by]=p.polygon[j];
      if((ay>y)!==(by>y) && x<(bx-ax)*(y-ay)/(by-ay)+ax)inside=!inside;
    }
    return inside;
  })||null;
}
export function paintBuildings(ctx,regions,sx,sy,scale,except){
  for(const r of regions||[]){if(r.tag===except)continue;
    for(const i of r.world?.installs||[]){ctx.fillStyle=i.role==='machine'?'#a6aaa0':'#747d78';ctx.fillRect(sx(i.x),sy(i.y),i.w*scale,i.h*scale);ctx.strokeStyle='#222c2c';ctx.lineWidth=.8;ctx.strokeRect(sx(i.x),sy(i.y),i.w*scale,i.h*scale);}
  }
}
