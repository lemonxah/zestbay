function bezierCtrl(graph, x1, x2) {
    return Math.max(32, Math.min(200, Math.abs(x2 - x1) / 2));
}

function isOrthoWire(graph) {
    return graph.wireStyle === "stepped" || graph.wireStyle === "circuit";
}

function wireLane(graph, wid) {
    if (graph.wireStyle !== "circuit" || wid === undefined || wid === null)
        return 0;
    return graph.wireLanes[wid] || 0;
}

function orthoPoints(graph, x1, y1, x2, y2, lane) {
    var stub = graph.wireStub;
    if (x2 - x1 >= 2 * stub) {
        if (Math.abs(y2 - y1) < 0.5)
            return [[x1, y1], [x2, y2]];
        var mid = Math.max(x1 + stub, Math.min(x2 - stub, (x1 + x2) / 2 + lane));
        return [[x1, y1], [mid, y1], [mid, y2], [x2, y2]];
    }
    var ym = Math.abs(y2 - y1) < 4 * stub ? Math.max(y1, y2) + 3 * stub + lane : (y1 + y2) / 2 + lane;
    var a = x1 + stub;
    var b = x2 - stub;
    return [[x1, y1], [a, y1], [a, ym], [b, ym], [b, y2], [x2, y2]];
}

function wirePathPoints(graph, x1, y1, x2, y2, wid) {
    var r = wid !== undefined && wid !== null ? graph.wireRoutes[wid] : null;
    if (r && r.x1 === x1 && r.y1 === y1 && r.x2 === x2 && r.y2 === y2)
        return r.pts;
    return graph.orthoPoints(x1, y1, x2, y2, graph.wireLane(wid));
}

function wireObstacles(graph) {
    var out = [];
    var m = graph.wireClearance;
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        var p = graph.nodePositions[n.id];
        if (!p)
            continue;
        var extra = n.type === "Subgraph" ? 4 : 0;
        out.push({
            l: p.x - m,
            t: p.y - m,
            r: p.x + graph.getNodeWidth(n.id) + extra + m,
            b: p.y + graph.calculateNodeHeight(n) + extra + m
        });
    }
    return out;
}

function segmentBlocked(graph, ax, ay, bx, by, obs, shrink) {
    var l = Math.min(ax, bx), r = Math.max(ax, bx), t = Math.min(ay, by), b = Math.max(ay, by);
    var e = 0.5 + (shrink || 0);
    for (var i = 0; i < obs.length; i++) {
        var o = obs[i];
        if (r > o.l + e && l < o.r - e && b > o.t + e && t < o.b - e)
            return true;
    }
    return false;
}

function pathBlocked(graph, pts, x1, y1, x2, y2, obs) {
    var n = pts.length;
    for (var i = 1; i < n; i++) {
        var ax = i === 1 ? Math.max(pts[0][0], x1 + graph.wireStub) : pts[i - 1][0];
        var bx = i === n - 1 ? Math.min(pts[n - 1][0], x2 - graph.wireStub) : pts[i][0];
        if (graph.segmentBlocked(ax, pts[i - 1][1], bx, pts[i][1], obs, 0))
            return true;
    }
    return false;
}

function uniqueSorted(graph, vals) {
    vals.sort(function (p, q) {
        return p - q;
    });
    var out = [];
    for (var i = 0; i < vals.length; i++) {
        if (out.length === 0 || vals[i] - out[out.length - 1] > 0.5)
            out.push(vals[i]);
    }
    var withMids = [];
    for (var k = 0; k < out.length; k++) {
        if (k > 0 && out[k] - out[k - 1] > 2 * graph.wireLaneGap)
            withMids.push((out[k] + out[k - 1]) / 2);
        withMids.push(out[k]);
    }
    return withMids;
}

function nearestIndex(graph, arr, v) {
    var best = 0;
    for (var i = 1; i < arr.length; i++) {
        if (Math.abs(arr[i] - v) < Math.abs(arr[best] - v))
            best = i;
    }
    return best;
}

function gridRoute(graph, sx, sy, ex, ey, obs) {
    var xs = [sx, ex];
    var ys = [sy, ey];
    var minX = Math.min(sx, ex), maxX = Math.max(sx, ex), minY = Math.min(sy, ey), maxY = Math.max(sy, ey);
    for (var oi = 0; oi < obs.length; oi++) {
        var o = obs[oi];
        xs.push(o.l, o.r);
        ys.push(o.t, o.b);
        minX = Math.min(minX, o.l);
        maxX = Math.max(maxX, o.r);
        minY = Math.min(minY, o.t);
        maxY = Math.max(maxY, o.b);
    }
    xs.push(minX - 3 * graph.wireLaneGap, maxX + 3 * graph.wireLaneGap);
    ys.push(minY - 3 * graph.wireLaneGap, maxY + 3 * graph.wireLaneGap);
    xs = graph.uniqueSorted(xs);
    ys = graph.uniqueSorted(ys);
    var nx = xs.length, ny = ys.length;
    var e = 0.5;
    function inside(x, y) {
        for (var q = 0; q < obs.length; q++) {
            var r = obs[q];
            if (x > r.l + e && x < r.r - e && y > r.t + e && y < r.b - e)
                return true;
        }
        return false;
    }
    var si = graph.nearestIndex(xs, sx), sj = graph.nearestIndex(ys, sy);
    var gi = graph.nearestIndex(xs, ex), gj = graph.nearestIndex(ys, ey);
    if (inside(xs[si], ys[sj]) || inside(xs[gi], ys[gj]))
        return null;
    if (si === gi && sj === gj)
        return [[sx, sy], [ex, ey]];
    var cells = nx * ny;
    var blockedPt = new Uint8Array(cells);
    var hOk = new Uint8Array(cells);
    var vOk = new Uint8Array(cells);
    for (var j = 0; j < ny; j++) {
        for (var i = 0; i < nx; i++) {
            var id = j * nx + i;
            blockedPt[id] = inside(xs[i], ys[j]) ? 1 : 0;
            if (i + 1 < nx)
                hOk[id] = inside((xs[i] + xs[i + 1]) / 2, ys[j]) ? 0 : 1;
            if (j + 1 < ny)
                vOk[id] = inside(xs[i], (ys[j] + ys[j + 1]) / 2) ? 0 : 1;
        }
    }
    var dist = new Float64Array(cells * 4).fill(Infinity);
    var prev = new Int32Array(cells * 4).fill(-1);
    var heap = [];
    function push(item) {
        heap.push(item);
        var c = heap.length - 1;
        while (c > 0) {
            var pnt = (c - 1) >> 1;
            if (heap[pnt][0] <= item[0])
                break;
            heap[c] = heap[pnt];
            c = pnt;
        }
        heap[c] = item;
    }
    function pop() {
        var top = heap[0];
        var last = heap.pop();
        if (heap.length > 0) {
            var c = 0;
            for (;;) {
                var l = 2 * c + 1, r = l + 1, m = c;
                if (l < heap.length && heap[l][0] < (m === c ? last[0] : heap[m][0]))
                    m = l;
                if (r < heap.length && heap[r][0] < (m === c ? last[0] : heap[m][0]))
                    m = r;
                if (m === c)
                    break;
                heap[c] = heap[m];
                c = m;
            }
            heap[c] = last;
        }
        return top;
    }
    function h(i, j) {
        return Math.abs(xs[i] - ex) + Math.abs(ys[j] - ey);
    }
    var bend = graph.wireBendCost;
    var start = (sj * nx + si) * 4;
    dist[start] = 0;
    push([h(si, sj), 0, start, 0]);
    var found = -1;
    while (heap.length > 0) {
        var it = pop();
        if (it[3] === 1) {
            found = it[2];
            break;
        }
        var st = it[2];
        if (it[1] > dist[st])
            continue;
        var cell = st >> 2, d = st & 3;
        var cj = Math.floor(cell / nx), ci = cell - cj * nx;
        for (var nd = 0; nd < 4; nd++) {
            if (nd === ((d + 2) & 3))
                continue;
            var ni = ci, nj = cj, ok = false, len = 0;
            if (nd === 0 && ci + 1 < nx && hOk[cell]) {
                ni = ci + 1;
                ok = true;
                len = xs[ni] - xs[ci];
            } else if (nd === 2 && ci > 0 && hOk[cell - 1]) {
                ni = ci - 1;
                ok = true;
                len = xs[ci] - xs[ni];
            } else if (nd === 1 && cj + 1 < ny && vOk[cell]) {
                nj = cj + 1;
                ok = true;
                len = ys[nj] - ys[cj];
            } else if (nd === 3 && cj > 0 && vOk[cell - nx]) {
                nj = cj - 1;
                ok = true;
                len = ys[cj] - ys[nj];
            }
            if (!ok)
                continue;
            var ncell = nj * nx + ni;
            if (blockedPt[ncell])
                continue;
            var ng = it[1] + len + (nd !== d ? bend : 0);
            var nst = ncell * 4 + nd;
            if (ng < dist[nst]) {
                dist[nst] = ng;
                prev[nst] = st;
                if (ni === gi && nj === gj)
                    push([ng + (nd !== 0 ? bend : 0), ng, nst, 1]);
                else
                    push([ng + h(ni, nj), ng, nst, 0]);
            }
        }
    }
    if (found < 0)
        return null;
    var path = [];
    for (var cur = found; cur >= 0; cur = prev[cur]) {
        var cc = cur >> 2;
        var pj = Math.floor(cc / nx);
        path.push([xs[cc - pj * nx], ys[pj]]);
    }
    path.reverse();
    path[0] = [sx, sy];
    path[path.length - 1] = [ex, ey];
    return path;
}

function simplifyOrtho(graph, pts) {
    var out = [];
    for (var i = 0; i < pts.length; i++) {
        var p = pts[i];
        if (out.length > 0 && Math.abs(out[out.length - 1][0] - p[0]) < 0.01 && Math.abs(out[out.length - 1][1] - p[1]) < 0.01)
            continue;
        if (out.length >= 2) {
            var a = out[out.length - 2], b = out[out.length - 1];
            if ((Math.abs(a[0] - b[0]) < 0.01 && Math.abs(b[0] - p[0]) < 0.01) || (Math.abs(a[1] - b[1]) < 0.01 && Math.abs(b[1] - p[1]) < 0.01)) {
                out[out.length - 1] = p;
                continue;
            }
        }
        out.push(p);
    }
    return out;
}

function routeOrtho(graph, x1, y1, x2, y2, obs) {
    var sx = x1 + graph.wireStub, ex = x2 - graph.wireStub;
    var pad = 160;
    var bx0 = Math.min(sx, ex) - pad, bx1 = Math.max(sx, ex) + pad;
    var by0 = Math.min(y1, y2) - pad, by1 = Math.max(y1, y2) + pad;
    var local = [];
    for (var i = 0; i < obs.length; i++) {
        var o = obs[i];
        if (o.r > bx0 && o.l < bx1 && o.b > by0 && o.t < by1)
            local.push(o);
    }
    var mid = graph.gridRoute(sx, y1, ex, y2, local);
    var full = mid ? graph.simplifyOrtho([[x1, y1]].concat(mid, [[x2, y2]])) : null;
    if ((!full || graph.pathBlocked(full, x1, y1, x2, y2, obs)) && local.length < obs.length) {
        mid = graph.gridRoute(sx, y1, ex, y2, obs);
        full = mid ? graph.simplifyOrtho([[x1, y1]].concat(mid, [[x2, y2]])) : null;
    }
    return full;
}

function planOrtho(graph, x1, y1, x2, y2, lane, obs, prev) {
    var simple = graph.orthoPoints(x1, y1, x2, y2, lane);
    if (!graph.pathBlocked(simple, x1, y1, x2, y2, obs))
        return {
            x1: x1,
            y1: y1,
            x2: x2,
            y2: y2,
            pts: simple,
            routed: false
        };
    var base = null;
    if (prev && prev.routed && prev.x1 === x1 && prev.y1 === y1 && prev.x2 === x2 && prev.y2 === y2 && !graph.pathBlocked(prev.base, x1, y1, x2, y2, obs))
        base = prev.base;
    else
        base = graph.routeOrtho(x1, y1, x2, y2, obs);
    if (!base)
        return null;
    return {
        x1: x1,
        y1: y1,
        x2: x2,
        y2: y2,
        base: base,
        pts: base.map(function (p) {
            return [p[0], p[1]];
        }),
        routed: true
    };
}

function nudgeRoutes(graph, routes, obs) {
    var groups = {};
    for (var r = 0; r < routes.length; r++) {
        var pts = routes[r].pts;
        for (var k = 1; k + 1 <= pts.length - 2; k++) {
            var a = pts[k], b = pts[k + 1];
            var vertical = Math.abs(a[0] - b[0]) < 0.01;
            var key = (vertical ? "v" : "h") + Math.round(vertical ? a[0] : a[1]);
            var lo = vertical ? Math.min(a[1], b[1]) : Math.min(a[0], b[0]);
            var hi = vertical ? Math.max(a[1], b[1]) : Math.max(a[0], b[0]);
            (groups[key] = groups[key] || []).push({
                pts: pts,
                k: k,
                vertical: vertical,
                lo: lo,
                hi: hi
            });
        }
    }
    for (var g in groups) {
        var segs = groups[g];
        if (segs.length < 2)
            continue;
        segs.sort(function (p, q) {
            return p.lo - q.lo;
        });
        var i = 0;
        while (i < segs.length) {
            var reach = segs[i].hi;
            var j = i + 1;
            while (j < segs.length && segs[j].lo <= reach + 1) {
                reach = Math.max(reach, segs[j].hi);
                j++;
            }
            var count = j - i;
            for (var c = 0; count > 1 && c < count; c++)
                graph.shiftSegment(segs[i + c], (c - (count - 1) / 2) * graph.wireLaneGap, obs);
            i = j;
        }
    }
}

function shiftSegment(graph, seg, offset, obs) {
    if (offset === 0)
        return;
    var pts = seg.pts, k = seg.k, axis = seg.vertical ? 0 : 1;
    var n = pts.length;
    var saved = [pts[k][axis], pts[k + 1][axis]];
    pts[k][axis] += offset;
    pts[k + 1][axis] += offset;
    var bad = pts[1][0] < pts[0][0] + graph.wireStub / 2 || pts[n - 2][0] > pts[n - 1][0] - graph.wireStub / 2;
    for (var s = Math.max(1, k); !bad && s <= Math.min(n - 1, k + 2); s++) {
        var ax = s === 1 ? pts[0][0] + graph.wireStub : pts[s - 1][0];
        var bx = s === n - 1 ? pts[n - 1][0] - graph.wireStub : pts[s][0];
        if (graph.segmentBlocked(ax, pts[s - 1][1], bx, pts[s][1], obs, graph.wireClearance - 2))
            bad = true;
    }
    if (bad) {
        pts[k][axis] = saved[0];
        pts[k + 1][axis] = saved[1];
    }
}

function updateWireRoutes(graph, positions) {
    if (!graph.isOrthoWire()) {
        graph.wireRoutes = {};
        return;
    }
    var obs = graph.wireObstacles();
    var prev = graph.wireRoutes;
    var next = {};
    for (var key in prev) {
        if (typeof key === "string" && (key.indexOf("b|") === 0 || key === "drag"))
            next[key] = prev[key];
    }
    var routed = [];
    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var a = positions[link.outputPortId];
        var b = positions[link.inputPortId];
        if (!a || !b || graph.isSnapInternal(a, b))
            continue;
        var r = graph.planOrtho(a.cx, a.cy, b.cx, b.cy, graph.wireLane(link.id), obs, prev[link.id]);
        if (!r)
            continue;
        next[link.id] = r;
        if (r.routed)
            routed.push(r);
    }
    if (graph.wireStyle === "circuit")
        graph.nudgeRoutes(routed, obs);
    graph.wireObstacleCache = obs;
    graph.wireRoutes = next;
}

function routeExtraWire(graph, wid, x1, y1, x2, y2) {
    if (!graph.isOrthoWire())
        return;
    var r = graph.planOrtho(x1, y1, x2, y2, 0, graph.wireObstacleCache, graph.wireRoutes[wid]);
    if (r)
        graph.wireRoutes[wid] = r;
    else
        delete graph.wireRoutes[wid];
}

function wirePolyline(graph, x1, y1, x2, y2, wid) {
    if (graph.wireStyle === "straight")
        return [[x1, y1], [x2, y2]];
    if (graph.isOrthoWire())
        return graph.wirePathPoints(x1, y1, x2, y2, wid);
    var c = graph.bezierCtrl(x1, x2);
    var cx1 = x1 + c;
    var cx2 = x2 - c;
    var pts = [[x1, y1]];
    var steps = 30;
    for (var i = 1; i <= steps; i++) {
        var t = i / steps;
        var u = 1 - t;
        pts.push([u * u * u * x1 + 3 * u * u * t * cx1 + 3 * u * t * t * cx2 + t * t * t * x2, u * u * u * y1 + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * y2]);
    }
    return pts;
}

function wireSamples(graph, x1, y1, x2, y2, wid, step) {
    var pts = graph.wirePolyline(x1, y1, x2, y2, wid);
    var out = [pts[0]];
    for (var i = 1; i < pts.length; i++) {
        var ax = pts[i - 1][0], ay = pts[i - 1][1];
        var dx = pts[i][0] - ax, dy = pts[i][1] - ay;
        var n = Math.max(1, Math.ceil(Math.sqrt(dx * dx + dy * dy) / step));
        for (var k = 1; k <= n; k++)
            out.push([ax + dx * k / n, ay + dy * k / n]);
    }
    return out;
}

function wireBounds(graph, x1, y1, x2, y2, wid) {
    if (!graph.isOrthoWire() && graph.wireStyle !== "straight") {
        var c = graph.bezierCtrl(x1, x2);
        return {
            x0: Math.min(x1, x2 - c),
            x1: Math.max(x1 + c, x2),
            y0: Math.min(y1, y2),
            y1: Math.max(y1, y2)
        };
    }
    var pts = graph.wirePolyline(x1, y1, x2, y2, wid);
    var b = {
        x0: Infinity,
        x1: -Infinity,
        y0: Infinity,
        y1: -Infinity
    };
    for (var i = 0; i < pts.length; i++) {
        b.x0 = Math.min(b.x0, pts[i][0]);
        b.x1 = Math.max(b.x1, pts[i][0]);
        b.y0 = Math.min(b.y0, pts[i][1]);
        b.y1 = Math.max(b.y1, pts[i][1]);
    }
    return b;
}

function wireMidpoint(graph, x1, y1, x2, y2, wid) {
    if (!graph.isOrthoWire())
        return {
            x: (x1 + x2) / 2,
            y: (y1 + y2) / 2
        };
    var pts = graph.wirePathPoints(x1, y1, x2, y2, wid);
    var total = 0;
    for (var i = 1; i < pts.length; i++)
        total += Math.abs(pts[i][0] - pts[i - 1][0]) + Math.abs(pts[i][1] - pts[i - 1][1]);
    var left = total / 2;
    for (var k = 1; k < pts.length; k++) {
        var len = Math.abs(pts[k][0] - pts[k - 1][0]) + Math.abs(pts[k][1] - pts[k - 1][1]);
        if (len >= left && len > 0) {
            var f = left / len;
            return {
                x: pts[k - 1][0] + (pts[k][0] - pts[k - 1][0]) * f,
                y: pts[k - 1][1] + (pts[k][1] - pts[k - 1][1]) * f
            };
        }
        left -= len;
    }
    return {
        x: (x1 + x2) / 2,
        y: (y1 + y2) / 2
    };
}

function traceWire(graph, ctx, x1, y1, x2, y2, wid) {
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    if (graph.wireStyle === "straight") {
        ctx.lineTo(x2, y2);
        return;
    }
    if (graph.isOrthoWire()) {
        var pts = graph.wirePathPoints(x1, y1, x2, y2, wid);
        var radius = graph.wireStyle === "circuit" ? graph.wireCorner : 0;
        for (var i = 1; i < pts.length - 1; i++) {
            var p = pts[i - 1], q = pts[i], n = pts[i + 1];
            var r = Math.min(radius, Math.hypot(q[0] - p[0], q[1] - p[1]) / 2, Math.hypot(n[0] - q[0], n[1] - q[1]) / 2);
            if (r > 0.5)
                ctx.arcTo(q[0], q[1], n[0], n[1], r);
            else
                ctx.lineTo(q[0], q[1]);
        }
        ctx.lineTo(x2, y2);
        return;
    }
    var c = graph.bezierCtrl(x1, x2);
    ctx.bezierCurveTo(x1 + c, y1, x2 - c, y2, x2, y2);
}

function strokeStyleOf(graph, color) {
    return color && color.addColorStop !== undefined ? color : "" + color;
}

function drawWire(graph, ctx, x1, y1, x2, y2, color, lineWidth, wid) {
    ctx.strokeStyle = graph.strokeStyleOf(color);
    ctx.lineWidth = lineWidth;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    graph.traceWire(ctx, x1, y1, x2, y2, wid);
    ctx.stroke();
}

function computeWireLanes(graph, positions) {
    var lanes = {};
    if (graph.wireStyle !== "circuit")
        return lanes;
    var entities = {};
    var list = [];
    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var a = positions[link.outputPortId];
        var b = positions[link.inputPortId];
        if (!a || !b || graph.isSnapInternal(a, b))
            continue;
        if (b.cx - a.cx < 2 * graph.wireStub || Math.abs(b.cy - a.cy) < 0.5)
            continue;
        var mid = (a.cx + b.cx) / 2;
        var key = link.outputPortId + "|" + Math.round(mid);
        var e = entities[key];
        if (!e) {
            e = {
                mid: mid,
                y1: a.cy,
                lo: a.cy,
                hi: a.cy,
                sumDy: 0,
                room: Infinity,
                links: []
            };
            entities[key] = e;
            list.push(e);
        }
        e.lo = Math.min(e.lo, b.cy);
        e.hi = Math.max(e.hi, b.cy);
        e.sumDy += b.cy - a.cy;
        e.room = Math.min(e.room, b.cx - a.cx - 2 * graph.wireStub);
        e.links.push(link.id);
    }
    list.sort(function (p, q) {
        return p.mid - q.mid || p.lo - q.lo;
    });
    var i = 0;
    while (i < list.length) {
        var j = i + 1;
        while (j < list.length && list[j].mid - list[j - 1].mid <= graph.wireLaneGap * 1.5)
            j++;
        graph.assignLaneGroup(list.slice(i, j), lanes);
        i = j;
    }
    return lanes;
}

function assignLaneGroup(graph, group, lanes) {
    group.sort(function (p, q) {
        return p.lo - q.lo;
    });
    var k = 0;
    while (k < group.length) {
        var reach = group[k].hi;
        var m = k + 1;
        while (m < group.length && group[m].lo <= reach + graph.wireLaneGap) {
            reach = Math.max(reach, group[m].hi);
            m++;
        }
        var comp = group.slice(k, m);
        comp.sort(function (p, q) {
            var pd = p.sumDy >= 0, qd = q.sumDy >= 0;
            if (pd !== qd)
                return pd ? -1 : 1;
            return pd ? q.y1 - p.y1 : p.y1 - q.y1;
        });
        var room = Infinity;
        for (var r = 0; r < comp.length; r++)
            room = Math.min(room, comp[r].room);
        var gap = comp.length > 1 ? Math.min(graph.wireLaneGap, room / (comp.length - 1)) : 0;
        for (var c = 0; c < comp.length; c++) {
            var offset = (c - (comp.length - 1) / 2) * gap;
            for (var l = 0; l < comp[c].links.length; l++)
                lanes[comp[c].links[l]] = offset;
        }
        k = m;
    }
}
