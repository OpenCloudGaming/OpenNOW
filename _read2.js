const fs=require('fs'); 
const l=fs.readFileSync('opennow-stable/src/renderer/src/gfn/webrtcClient.ts','utf8').split(/\r?\n/); 
for(let i=1360;i<1480;i++)console.log((i+1)+': '+l[i]); 
