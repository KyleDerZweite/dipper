// Only the uniquely matched owned quick window can move. No focus changes.
var pid = __PID__;
var service = __SERVICE__;
var hint = __POINT__;
var finished = false;
function finish(placed) {
  if (finished) return;
  finished = true;
  callDBus(service, "/org/dipper/QuickPlacement", "org.dipper.QuickPlacement", "Report", placed);
}
function place() {
  var windows = workspace.stackingOrder;
  if (windows.length > 4096) { finish(false); return true; }
  var matches = windows.filter(function (w) { return w.pid === pid && String(w.caption) === "Dipper Quick Panel"; });
  if (!matches.length) return false;
  if (matches.length !== 1) { finish(false); return true; }
  var point = hint[0] > 0 || hint[1] > 0 ? {x:hint[0], y:hint[1]} : workspace.cursorPos;
  var screens = workspace.screens;
  if (!point || !screens || screens.length > 32) { finish(false); return true; }
  var screen = screens.find(function (s) { var r=s.geometry; return point.x>=r.x && point.x<r.x+r.width && point.y>=r.y && point.y<r.y+r.height; });
  if (!screen) { finish(false); return true; }
  var area = screen.geometry;
  var w = matches[0], rect = w.frameGeometry;
  if (rect.width > area.width-16 || rect.height > area.height-16) { finish(false); return true; }
  var x = Math.max(area.x+8, Math.min(point.x-rect.width+20, area.x+area.width-rect.width-8));
  var y = point.y < area.y+area.height/2 ? point.y+12 : point.y-rect.height-12;
  y = Math.max(area.y+8, Math.min(y, area.y+area.height-rect.height-8));
  w.frameGeometry = {x:x, y:y, width:rect.width, height:rect.height};
  finish(true);
  return true;
}
if (!place()) workspace.windowAdded.connect(function () { if (!finished) place(); });
