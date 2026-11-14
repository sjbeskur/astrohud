import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const scope = {};
vm.runInNewContext(readFileSync(new URL('../../astrohud-rest/static/display.js', import.meta.url), 'utf8') + '\nglobalThis.Order = MemoryLanePlaybackOrder;', scope);
const {Order} = scope;
const saved = new Map();
const storage = {getItem: key => saved.get(key), setItem: (key, value) => saved.set(key,value)};
const ids = ['new','middle','old'];
let seed=7;const random=()=>((seed=(seed*1664525+1013904223)>>>0)/4294967296);
let order = new Order(random,storage);
for(let round=0;round<10;round++) {
  const shown=new Set();
  for(let i=0;i<ids.length;i++) {
    order.reconcile('frame','scenic_route',ids);
    const id=order.peek(); assert.equal(order.peek(),id);
    assert(!shown.has(id)); shown.add(id); order.shown(id);
    // Reconstruct from actual saved browser progress mid-round.
    order=new Order(random,storage);
  }
  assert.equal(shown.size,3);
}
order.reconcile('frame','scenic_route',ids);
order.consumeOwnerRevision(9);
order = new Order(random, storage);
order.reconcile('frame','fresh_prints',ids);
assert.equal(order.ownerRevision,9);
for(const expected of ['new','middle','old','new']) {assert.equal(order.peek(),expected);order.shown(expected);}
order.reconcile('frame','serendipity',ids);
let previous,repeat=false;const seen=new Set();
for(let i=0;i<100;i++){const id=order.peek();repeat ||= id===previous;seen.add(id);order.shown(id);previous=id;}
assert(repeat);assert.equal(seen.size,3);
order.reconcile('frame','scenic_route',ids);
const first=order.peek();order.shown(first);
order.reconcile('frame','scenic_route',[...ids,'arriving'].filter(id=>id!==first));
const remaining=new Set();for(let i=0;i<3;i++){const id=order.peek();assert.notEqual(id,first);remaining.add(id);order.shown(id);}
assert.equal(remaining.size,3);assert(remaining.has('arriving'));
order.reconcile('other','scenic_route',[]);assert.equal(order.peek(),null);
order.reconcile('other','scenic_route',['only']);for(let i=0;i<4;i++){assert.equal(order.peek(),'only');order.shown('only');}
const noStorage = new Order(()=>0,{getItem(){throw Error('blocked');},setItem(){throw Error('blocked');}});
noStorage.reconcile('private','serendipity',ids);noStorage.shown(noStorage.peek());assert.equal(noStorage.peek(),'new');
console.log('PASS: browser playback rounds, persisted progress, prefetch stability, mode changes, random repeats, membership changes, empty/single and unavailable storage');
