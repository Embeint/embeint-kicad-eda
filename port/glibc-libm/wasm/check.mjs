// SPDX-License-Identifier: GPL-3.0-or-later
import { readFileSync } from 'node:fs';
const [modulePath, recordsPath, countText] = process.argv.slice(2);
const module = await WebAssembly.compile(readFileSync(modulePath));
if (WebAssembly.Module.imports(module).length !== 0) throw new Error('wasm has imports');
const instance = await WebAssembly.instantiate(module, {});
const data=readFileSync(recordsPath);
const count=Number(countText);
if(data.length!==count*56) throw new Error('record count');
const names=['atan','atan2','tan','asin','acos'];
const failures=[0,0,0,0,0];
for(let i=0;i<count;i++) {
  const b=i*56,x=data.readBigUInt64LE(b),y=data.readBigUInt64LE(b+8);
  for(let fn=0;fn<5;fn++) {
    const got=BigInt.asUintN(64,instance.exports.angle_libm(x,y,fn));
    const want=data.readBigUInt64LE(b+16+fn*8);
    if(got!==want && failures[fn]++<10) console.log(`${names[fn]} x ${x.toString(16)} y ${y.toString(16)} got ${got.toString(16)} want ${want.toString(16)}`);
  }
}
names.forEach((fn,i)=>console.log(`glibc_libm ${fn}: ${count} records, ${failures[i]} mismatches`));
process.exit(failures.every(n=>n===0)?0:1);
