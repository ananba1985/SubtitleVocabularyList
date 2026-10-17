import assert from 'node:assert/strict';
import {applyTaskUpdate, waitForTask} from '../src/taskUpdates.ts';

const task = (id, state, updatedAt, visibleInList = true) => ({
  id, operationId: id, kind: 'import', stage: state, state, current: 0, total: 1,
  message: state, result: null, error: null, createdAt: 1, updatedAt, visibleInList,
});
let tasks = applyTaskUpdate([], task('first', 'running', 1));
tasks = applyTaskUpdate(tasks, task('second', 'running', 2));
assert.deepEqual(tasks.map(value => value.id), ['second', 'first']);
tasks = applyTaskUpdate(tasks, task('first', 'succeeded', 3));
tasks = applyTaskUpdate(tasks, {...task('speech', 'succeeded', 4, false), kind: 'speech'});
assert.equal(tasks.length, 2);
tasks = applyTaskUpdate(tasks, task('second', 'succeeded', 5));
assert.deepEqual(tasks.map(value => value.id), ['second']);

let receive;
let reads = 0;
let stops = 0;
const seen = [];
const finished = waitForTask(task('audio', 'queued', 1),
  async () => { reads++; return task('audio', 'running', 2); },
  async callback => { receive = callback; return () => stops++; },
  value => seen.push(value.state));
await new Promise(resolve => setImmediate(resolve));
receive({...task('audio', 'running', 3), current: 1});
receive({...task('audio', 'succeeded', 4), result: {path: 'synthetic.wav'}});
assert.deepEqual(await finished, {path: 'synthetic.wav'});
assert.equal(reads, 1);
assert.equal(stops, 1);
assert.equal(seen.at(-1), 'succeeded');
console.log('PASS task events update active/latest views and complete one subscribed operation with one snapshot read and listener cleanup.');
