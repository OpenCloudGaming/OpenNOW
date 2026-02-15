const fs=require('fs'); 
const l=fs.readFileSync('opennow-stable/src/renderer/src/gfn/inputProtocol.ts','utf8').split(/\r?\n/); 
for(let i=500;i<560;i++)console.log((i+1)+': '+l[i]); 
