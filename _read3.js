const fs=require('fs'); 
const l=fs.readFileSync('opennow-stable/src/renderer/src/gfn/inputProtocol.ts','utf8').split(/\r?\n/); 
for(let i=1;i<280;i++)console.log((i+1)+': '+l[i]); 
