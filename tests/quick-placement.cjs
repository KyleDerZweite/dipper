const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync('apps/desktop/src/quick_placement.js', 'utf8').replace('__PID__', '123').replace('__SERVICE__', '"fixture"').replace('__POINT__', '[4400,1400]');
const quick = () => ({pid:123,caption:'Dipper Quick Panel',frameGeometry:{x:0,y:0,width:320,height:360}});
function run(windows) {
  const reports = [];
  const workspace = { stackingOrder:windows, screens:[{geometry:{x:0,y:0,width:1920,height:1080}},{geometry:{x:1920,y:0,width:2560,height:1440}}], windowAdded:{connect() {}} };
  vm.runInNewContext(source, { workspace, callDBus: (...args) => reports.push(args.at(-1)) });
  return reports;
}
test('placement reads only owned titles and stays inside the tray output', () => {
  const unrelated = {pid:999,get caption() {assert.fail('must not inspect unrelated window titles');}};
  const panel = quick();
  assert.deepEqual(run([unrelated,panel]), [true]);
  assert.ok(panel.frameGeometry.x>=1920 && panel.frameGeometry.x+320<=4480);
  assert.ok(panel.frameGeometry.y>=0 && panel.frameGeometry.y+360<1400);
});
test('ambiguous owned quick windows never move', () => {
  const first=quick(),second=quick();
  assert.deepEqual(run([first,second]),[false]);
  assert.equal(first.frameGeometry.x,0);
  assert.equal(second.frameGeometry.x,0);
});
