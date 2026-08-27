/* frankenmermaid deck runtime (epic bd-z7g6k, bd-lnca7, morph bd-tm1q7).
 *
 * Presents a rendered frankenmermaid SVG as a graphcon-deck-style slideshow, driven entirely
 * by the engine's DeckManifest: camera fit + tween per slide, dim/half focus choreography,
 * staggered step reveals, an overview scene with a window-replay tour, free camera, click-a-
 * dimmed-node travel, tooltips, autoplay, and keyboard control — with zero graph logic of its
 * own (membership, steps, ordering, and geometry are all precomputed in the manifest).
 *
 * MORPH MODE (manifest schema >= 1.1.0, `nodeGeometry` + `edgeEndpoints` present): the graph
 * itself rearranges, graphcon-deck's signature look. Every node floats on a gentle per-node
 * sine bob; on slide change, off-slide nodes GLIDE OUT past the fitted camera window edge
 * (ray-scaled push-out, eased per frame) while members glide home; edges are redrawn every
 * frame as straight paths between their endpoints' live border points (engine paths park
 * hidden; edge labels ride along on a group translate); member nodes are draggable with
 * spring-back. Falls back to the static choreography under prefers-reduced-motion or a
 * pre-1.1 manifest. The loop runs continuously ONLY while the stage is on screen
 * (IntersectionObserver-gated) — a background section still burns zero frames.
 *
 * Canonical copy: crates/fm-cli/src/deck_runtime.js (embedded into the CLI's talk.html via
 * include_str!; inlined into the showcase between deck-runtime markers; deployed to
 * /web/fm-deck-runtime.js — all byte-guarded by scripts/verify_deck_runtime.mjs).
 *
 * Zero dependencies. ES-module-safe AND classic-script-safe: no imports, no exports — the
 * single global is window.FmDeckRuntime. All deck-sourced text reaches the DOM via
 * textContent (deck text is untrusted; never innerHTML).
 */
(function () {
  "use strict";

  var CAMERA_LERP = 0.085; // per-frame approach factor, graphcon's feel
  var SNAP_EPSILON = 0.1; // px: snap-to-target threshold that lets the loop park
  var STAGGER_MS = 90; // per-element reveal interval within a step
  var TOUR_MOVE_MS = 700;
  var TOUR_PAUSE_MS = 900;
  // Morph-mode tuning, ported from graphcon-deck's render loop so the feel matches.
  var FLOAT_AMP = 3.5; // viewBox units of idle bob
  var FLOAT_SPEED_X = 0.00055;
  var FLOAT_SPEED_Y = 0.00045;
  var PUSH_LERP = 0.06; // per-frame approach toward the push-out target
  var PUSH_MARGIN = 160; // extra world units past the camera window that exiles must clear
  var DRAG_RETURN = 0.88; // spring-back decay for view-mode node drags

  function clamp(value, low, high) {
    return Math.min(high, Math.max(low, value));
  }

  // Some hosts hand the manifest across a boundary that turns Record fields into JS Maps
  // (older wasm builds did exactly that); normalize so lookups stay plain property access.
  function asRecord(value) {
    if (!value) return null;
    if (typeof Map === "function" && value instanceof Map) {
      var out = Object.create(null);
      value.forEach(function (entry, key) {
        out[key] = entry;
      });
      return out;
    }
    return value;
  }

  function reducedMotion() {
    try {
      return (
        typeof matchMedia === "function" &&
        matchMedia("(prefers-reduced-motion: reduce)").matches
      );
    } catch (_error) {
      return false;
    }
  }

  function mount(options) {
    var stage = options.stage;
    var manifest = options.manifest;
    var ui = options.ui || {};
    var onSlideChange = options.onSlideChange || function () {};
    if (!stage || !manifest || !Array.isArray(manifest.slides)) {
      throw new Error("FmDeckRuntime.mount needs { stage, svg, manifest }");
    }

    /* ── DOM scaffolding ─────────────────────────────────────────── */

    var viewport = document.createElement("div");
    viewport.className = "fm-deck-viewport";
    viewport.style.position = "absolute";
    viewport.style.left = "0";
    viewport.style.top = "0";
    viewport.style.transformOrigin = "0 0";
    viewport.style.willChange = "transform";
    if (typeof options.svg === "string") {
      viewport.innerHTML = options.svg; // engine-produced SVG, not deck text
    } else if (options.svg) {
      viewport.appendChild(options.svg);
    }
    var svgRoot = viewport.querySelector("svg");
    if (!svgRoot) {
      throw new Error("FmDeckRuntime.mount: no <svg> in the provided content");
    }
    stage.appendChild(viewport);

    // PIN THE SVG (coordinate contract): the renderer's responsive mode emits width="100%",
    // which letterboxes the SVG inside its CSS box and silently breaks every fit computation.
    // Pinning style width/height to the viewBox establishes 1 SVG user unit == 1 CSS px at
    // scale 1 — the same normalization the showcase PanZoomController performs.
    var viewBox = svgRoot.viewBox && svgRoot.viewBox.baseVal;
    var worldWidth = viewBox && viewBox.width ? viewBox.width : manifest.viewBox.width;
    var worldHeight = viewBox && viewBox.height ? viewBox.height : manifest.viewBox.height;
    svgRoot.style.width = worldWidth + "px";
    svgRoot.style.height = worldHeight + "px";
    svgRoot.style.maxWidth = "none";
    svgRoot.style.maxHeight = "none";
    svgRoot.style.display = "block";

    // Focus/reveal classes. One style element per mount; dim level from the manifest.
    var style = document.createElement("style");
    style.textContent =
      ".fm-deck-viewport .fm-deck-dim{opacity:var(--fm-deck-dim,.07);transition:opacity .5s ease;}" +
      ".fm-deck-viewport .fm-deck-half{opacity:.45;transition:opacity .5s ease;}" +
      ".fm-deck-viewport .fm-deck-hidden{opacity:0 !important;transition:opacity .5s ease;}" +
      ".fm-deck-viewport [id]{transition:opacity .5s ease;}" +
      // Morph mode: engine edge paths park (live paths replace them); exiled nodes stay
      // half-visible so their flight out of the window reads as motion, not a fade.
      ".fm-deck-morphing .fm-deck-parked{visibility:hidden;}" +
      ".fm-deck-viewport .fm-deck-live path{transition:opacity .5s ease;}" +
      '.fm-deck-viewport.fm-deck-morphing g[id^="fm-node-"].fm-deck-dim{opacity:.55;}' +
      ".fm-deck-tip{position:absolute;z-index:30;max-width:270px;background:#1d1f23;color:#f3f1ea;" +
      "font-size:11.5px;line-height:1.5;padding:9px 12px;border-radius:9px;pointer-events:none;" +
      "opacity:0;transition:opacity .18s ease;}" +
      ".fm-deck-tip.fm-deck-tip-show{opacity:.96;}";
    stage.appendChild(style);
    stage.style.setProperty(
      "--fm-deck-dim",
      String(clamp(manifest.options.dimOpacity, 0, 1))
    );

    // A11y: the stage is a slideshow region; slide changes are announced politely.
    stage.setAttribute("role", "region");
    stage.setAttribute("aria-roledescription", "slideshow");
    if (manifest.title) stage.setAttribute("aria-label", manifest.title);
    var liveRegion = document.createElement("div");
    liveRegion.setAttribute("aria-live", "polite");
    liveRegion.style.position = "absolute";
    liveRegion.style.width = "1px";
    liveRegion.style.height = "1px";
    liveRegion.style.overflow = "hidden";
    liveRegion.style.clipPath = "inset(50%)";
    stage.appendChild(liveRegion);

    var tip = document.createElement("div");
    tip.className = "fm-deck-tip";
    stage.appendChild(tip);

    /* ── Element registry (SCOPED to this SVG) ───────────────────── */
    // Never document.getElementById: element ids are deterministic PER DIAGRAM and the host
    // page may render the same diagram elsewhere (the showcase theater after Open-in-Theater).

    var wanted = Object.create(null); // elementId -> true
    var tooltipOf = Object.create(null); // elementId -> tip text
    var slideIndexOfId = asRecord(manifest.nodeSlideIndex) || {};
    manifest.slides.forEach(function (slide) {
      slide.nodes.forEach(function (node) {
        wanted[node.elementId] = true;
        if (node.tooltip) tooltipOf[node.elementId] = node.tooltip;
      });
      (slide.edges || []).forEach(function (edge) {
        wanted[edge.elementId] = true;
      });
      (slide.clusters || []).forEach(function (cluster) {
        wanted[cluster.elementId] = true;
      });
    });
    // Morph mode needs the WHOLE graph, not just slide members — exiles fly too.
    var geometryOf = asRecord(manifest.nodeGeometry);
    var endpointsOf = asRecord(manifest.edgeEndpoints);
    if (geometryOf) {
      Object.keys(geometryOf).forEach(function (id) {
        wanted[id] = true;
      });
    }
    if (endpointsOf) {
      Object.keys(endpointsOf).forEach(function (id) {
        wanted[id] = true;
      });
    }
    var elements = Object.create(null); // elementId -> Element
    var all = svgRoot.querySelectorAll("[id]");
    for (var i = 0; i < all.length; i += 1) {
      var candidate = all[i];
      if (wanted[candidate.id]) elements[candidate.id] = candidate;
    }

    /* ── Morph model (graphcon parity, bd-tm1q7) ─────────────────── */

    var morphEnabled =
      !reducedMotion() &&
      !!geometryOf &&
      !!endpointsOf &&
      Object.keys(geometryOf).length > 0;
    var morphNodes = Object.create(null); // elementId -> node motion state
    var morphNodeIds = [];
    var liveEdges = [];
    var liveOf = Object.create(null); // engine edge group id -> live path (class mirror)
    var pushCtx = null; // {cx, cy, rw, rh, margin} in viewBox space
    var pushMembers = Object.create(null); // elementId -> true (home team this scene)
    var stageOnScreen = true;

    // Rect-clip border point: where the segment from n's center toward (tx, ty) exits n's
    // box (+4 breathing room), so live edges land on node borders, not centers.
    function borderPoint(node, towardX, towardY) {
      var dx = towardX - node.fx;
      var dy = towardY - node.fy;
      if (!dx && !dy) return [node.fx, node.fy];
      var hw = node.hw + 4;
      var hh = node.hh + 4;
      var scale = Math.min(
        dx ? hw / Math.abs(dx) : Infinity,
        dy ? hh / Math.abs(dy) : Infinity,
        1
      );
      return [node.fx + dx * scale, node.fy + dy * scale];
    }

    if (morphEnabled) {
      viewport.classList.add("fm-deck-morphing");
      Object.keys(geometryOf).forEach(function (id, index) {
        var el = elements[id];
        var rect = geometryOf[id];
        if (!el || !rect) return;
        morphNodes[id] = {
          id: id,
          el: el,
          hx: rect.x + rect.width / 2, // home center
          hy: rect.y + rect.height / 2,
          hw: rect.width / 2,
          hh: rect.height / 2,
          phase: (index * 1.83) % 6.283,
          px: 0, // eased push-out displacement
          py: 0,
          tx: 0, // temporary drag offset (springs back)
          ty: 0,
          fx: rect.x + rect.width / 2, // current rendered center, filled per frame
          fy: rect.y + rect.height / 2,
          dragging: false,
        };
        morphNodeIds.push(id);
      });
      // Live edge layer sits under the node groups (over clusters), inheriting the camera.
      var liveLayer = document.createElementNS("http://www.w3.org/2000/svg", "g");
      liveLayer.setAttribute("class", "fm-deck-live");
      var firstNodeGroup = svgRoot.querySelector('g[id^="fm-node-"]');
      if (firstNodeGroup && firstNodeGroup.parentNode) {
        firstNodeGroup.parentNode.insertBefore(liveLayer, firstNodeGroup);
      } else {
        svgRoot.appendChild(liveLayer);
      }
      Object.keys(endpointsOf).forEach(function (id) {
        var group = elements[id];
        var ends = endpointsOf[id];
        var from = ends && morphNodes[ends.fromElementId];
        var to = ends && morphNodes[ends.toElementId];
        if (!group || !from || !to) return;
        // Engine edges are groups wrapping a path; hand-authored fixtures may use bare paths.
        var enginePath =
          group.tagName === "path" ? group : group.querySelector("path");
        if (!enginePath) return;
        var live = document.createElementNS("http://www.w3.org/2000/svg", "path");
        // Inherit the engine path's look (theme classes, width, dash, arrow markers).
        var carry = ["class", "stroke", "stroke-width", "stroke-dasharray", "marker-start", "marker-end"];
        for (var c = 0; c < carry.length; c += 1) {
          var value = enginePath.getAttribute(carry[c]);
          if (value !== null) live.setAttribute(carry[c], value);
        }
        live.setAttribute("fill", "none");
        liveLayer.appendChild(live);
        enginePath.classList.add("fm-deck-parked"); // engine path hides; label stays
        var ax = borderPoint(from, to.fx, to.fy);
        var bx = borderPoint(to, from.fx, from.fy);
        liveEdges.push({
          group: group,
          live: live,
          from: from,
          to: to,
          // Home midpoint: the label (still inside the engine group) rides a group
          // translate of (current midpoint − this).
          mx0: (ax[0] + bx[0]) / 2,
          my0: (ax[1] + bx[1]) / 2,
        });
        liveOf[id] = live;
      });
    }

    // Push-out target, graphcon's pushTarget: scale the home→center ray until the node
    // clears the camera window rect (+ its own half-extents + margin). Members stay put.
    function pushTargetOf(node) {
      if (!pushCtx || pushMembers[node.id]) return [0, 0];
      var dx = node.hx - pushCtx.cx;
      var dy = node.hy - pushCtx.cy;
      if (!dx && !dy) {
        dx = 0;
        dy = 1;
      }
      var reachW = pushCtx.rw + node.hw + pushCtx.margin;
      var reachH = pushCtx.rh + node.hh + pushCtx.margin;
      var m = Math.max(Math.abs(dx) / reachW, Math.abs(dy) / reachH);
      if (m >= 1) return [0, 0]; // already outside the window
      var k = 1 / m;
      return [dx * (k - 1), dy * (k - 1)];
    }

    /* ── Scenes ──────────────────────────────────────────────────── */
    // Scene list = slides + (optionally) the overview. The overview's index is
    // manifest.slides.length and its `slide` is null in onSlideChange.

    var overviewEnabled = manifest.overview && manifest.overview.enabled !== false;
    var sceneCount = manifest.slides.length + (overviewEnabled ? 1 : 0);
    var state = {
      scene: 0,
      step: 0,
      freeCam: false,
      cam: { x: worldWidth / 2, y: worldHeight / 2, s: 0.5 },
      target: null,
      raf: 0,
      parked: true,
      staggerTimers: [],
      tour: null, // {index, phase, from, to, phaseStart}
      tourRect: null,
      autoplayTimer: 0,
      autoplayPausedUntil: 0,
      destroyed: false,
    };

    function isOverview(sceneIndex) {
      return overviewEnabled && sceneIndex === manifest.slides.length;
    }

    function currentSlide() {
      return isOverview(state.scene) ? null : manifest.slides[state.scene];
    }

    function maxStepOf(sceneIndex) {
      var slide = isOverview(sceneIndex) ? null : manifest.slides[sceneIndex];
      return slide ? slide.maxStep : 0;
    }

    /* ── Camera ──────────────────────────────────────────────────── */

    function stageSize() {
      return { w: stage.clientWidth || 1, h: stage.clientHeight || 1 };
    }

    function fitTarget(bounds, margin, zoomMax) {
      var size = stageSize();
      var scale = Math.min(
        size.w / (bounds.width + margin * 2),
        size.h / (bounds.height + margin * 2),
        zoomMax
      );
      return {
        x: bounds.x + bounds.width / 2,
        y: bounds.y + bounds.height / 2,
        s: Math.max(scale, 0.01),
      };
    }

    // graphcon's computeFit skips hidden nodes: the camera frames what is VISIBLE at the
    // current step and zooms out as reveals land, instead of framing voids where
    // still-hidden members will eventually appear. Needs home geometry (morph manifests);
    // slides without it, and edge/cluster-only extents, fall back to the engine bounds.
    function visibleMemberBounds(sceneIndex) {
      if (!morphEnabled || isOverview(sceneIndex)) return null;
      var slide = manifest.slides[sceneIndex];
      var x0 = Infinity;
      var y0 = Infinity;
      var x1 = -Infinity;
      var y1 = -Infinity;
      var any = false;
      slide.nodes.forEach(function (node) {
        if (node.step > state.step) return;
        var rect = geometryOf[node.elementId];
        if (!rect) return;
        any = true;
        x0 = Math.min(x0, rect.x);
        y0 = Math.min(y0, rect.y);
        x1 = Math.max(x1, rect.x + rect.width);
        y1 = Math.max(y1, rect.y + rect.height);
      });
      if (!any) return null;
      return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
    }

    function sceneCameraTarget(sceneIndex) {
      if (isOverview(sceneIndex)) {
        return fitTarget(manifest.viewBox, manifest.options.fitMargin, manifest.options.zoomMax);
      }
      var slide = manifest.slides[sceneIndex];
      var bounds = visibleMemberBounds(sceneIndex) || slide.bounds;
      return fitTarget(bounds, slide.fitMargin, slide.zoomMax);
    }

    function applyCamera() {
      var size = stageSize();
      viewport.style.transform =
        "translate3d(" +
        (size.w / 2 - state.cam.x * state.cam.s) +
        "px, " +
        (size.h / 2 - state.cam.y * state.cam.s) +
        "px, 0) scale(" +
        state.cam.s +
        ")";
    }

    // The camera window every exile must clear, in viewBox space at the TARGET zoom (the
    // fit the camera is heading to, not wherever the tween currently is).
    function updatePushCtx() {
      if (!morphEnabled) return;
      if (isOverview(state.scene)) {
        pushCtx = null; // overview: everyone glides home (the tour retargets it per stop)
        return;
      }
      var target = sceneCameraTarget(state.scene);
      var size = stageSize();
      pushCtx = {
        cx: target.x,
        cy: target.y,
        rw: size.w / 2 / target.s,
        rh: size.h / 2 / target.s,
        margin: PUSH_MARGIN,
      };
    }

    function retarget() {
      if (state.freeCam) return;
      state.target = sceneCameraTarget(state.scene);
      updatePushCtx();
      if (reducedMotion()) {
        state.cam = { x: state.target.x, y: state.target.y, s: state.target.s };
        state.target = null;
        applyCamera();
        return;
      }
      wake();
    }

    /* ── Focus + steps ───────────────────────────────────────────── */

    function classify(el, className, on) {
      if (!el) return;
      if (on) el.classList.add(className);
      else el.classList.remove(className);
      // Live edge paths mirror their engine group's focus classes (single choke point:
      // every dim/half/hidden change anywhere flows through here).
      var live = el.id && liveOf[el.id];
      if (live) {
        if (on) live.classList.add(className);
        else live.classList.remove(className);
      }
    }

    function applyScene(stagger) {
      clearStagger();
      var overview = isOverview(state.scene);
      var slide = currentSlide();
      var inScene = Object.create(null); // elementId -> {step, half}
      if (overview) {
        Object.keys(elements).forEach(function (id) {
          inScene[id] = { step: 0, half: false };
        });
      } else if (slide) {
        slide.nodes.forEach(function (node) {
          inScene[node.elementId] = { step: node.step, half: false };
        });
        (slide.edges || []).forEach(function (edge) {
          inScene[edge.elementId] = { step: edge.step, half: !!edge.touching };
        });
        (slide.clusters || []).forEach(function (cluster) {
          inScene[cluster.elementId] = {
            step: cluster.step,
            half: !cluster.cameraContained,
            cluster: true,
          };
        });
      }
      // Morph home team: nodes that stay put this scene (revealed members). Everyone else
      // gets pushed out past the camera window.
      pushMembers = Object.create(null);
      var freshlyRevealed = [];
      Object.keys(elements).forEach(function (id) {
        var el = elements[id];
        var membership = inScene[id];
        if (!membership) {
          classify(el, "fm-deck-dim", true);
          classify(el, "fm-deck-half", false);
          classify(el, "fm-deck-hidden", false);
          return;
        }
        classify(el, "fm-deck-dim", false);
        classify(el, "fm-deck-half", membership.half);
        var hidden = membership.step > state.step;
        // A partially-contained cluster box is sized around members that morph mode has
        // flown away — the box would lie, so it hides instead of rendering half-dim.
        if (morphEnabled && !overview && membership.cluster && membership.half) {
          hidden = true;
        }
        if (!hidden && morphNodes[id]) pushMembers[id] = true;
        if (!hidden && stagger && membership.step === state.step && state.step > 0) {
          freshlyRevealed.push(id);
          hidden = true; // reveal below, staggered
        }
        classify(el, "fm-deck-hidden", hidden);
      });
      if (freshlyRevealed.length && slide) {
        // Reveal in the ENGINE's precomputed order (steps[].elementIds), never our own.
        var stepList = null;
        for (var s = 0; s < (slide.steps || []).length; s += 1) {
          if (slide.steps[s].step === state.step) stepList = slide.steps[s].elementIds;
        }
        var order = stepList || freshlyRevealed;
        var interval = reducedMotion() ? 0 : STAGGER_MS;
        order.forEach(function (id, position) {
          if (!elements[id]) return;
          var timer = setTimeout(function () {
            classify(elements[id], "fm-deck-hidden", false);
          }, interval * position);
          state.staggerTimers.push(timer);
        });
      }
      updateChrome();
    }

    function clearStagger() {
      state.staggerTimers.forEach(clearTimeout);
      state.staggerTimers = [];
    }

    /* ── Chrome (host-provided UI elements) ──────────────────────── */

    var dots = [];
    if (ui.dots) {
      ui.dots.textContent = "";
      for (var d = 0; d < sceneCount; d += 1) {
        var dot = document.createElement("i");
        dot.setAttribute("role", "button");
        dot.setAttribute("tabindex", "0");
        (function (sceneIndex) {
          dot.addEventListener("click", function () {
            api.go(sceneIndex);
          });
        })(d);
        ui.dots.appendChild(dot);
        dots.push(dot);
      }
    }

    function pad(number) {
      return (number < 10 ? "0" : "") + number;
    }

    function updateChrome() {
      var slide = currentSlide();
      var title = slide ? slide.title : (manifest.overview && manifest.overview.title) || "Overview";
      var caption = slide ? slide.caption || "" : (manifest.overview && manifest.overview.caption) || "";
      if (ui.title) ui.title.textContent = title;
      if (ui.caption) ui.caption.textContent = caption;
      if (ui.num) {
        var text = pad(state.scene + 1) + " / " + pad(sceneCount);
        var max = maxStepOf(state.scene);
        if (max > 0) text += " · " + state.step + "/" + max;
        ui.num.textContent = text;
      }
      dots.forEach(function (dot, index) {
        if (index === state.scene) dot.setAttribute("data-active", "true");
        else dot.removeAttribute("data-active");
      });
      liveRegion.textContent = caption ? title + " — " + caption : title;
    }

    /* ── Tour (overview finale) ──────────────────────────────────── */

    function slideWindow(index) {
      var slide = manifest.slides[index];
      return {
        x: slide.bounds.x - slide.fitMargin / 2,
        y: slide.bounds.y - slide.fitMargin / 2,
        w: slide.bounds.width + slide.fitMargin,
        h: slide.bounds.height + slide.fitMargin,
      };
    }

    function startTour() {
      if (!manifest.overview || manifest.overview.tour === false) return;
      if (!manifest.slides.length) return;
      if (!state.tourRect) {
        var rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        rect.setAttribute("class", "fm-deck-tour");
        rect.setAttribute("fill", "none");
        rect.setAttribute("stroke", "#34d399");
        rect.setAttribute("stroke-width", "2.5");
        rect.setAttribute("vector-effect", "non-scaling-stroke");
        rect.setAttribute("rx", "10");
        rect.style.cursor = "pointer";
        rect.setAttribute("tabindex", "0");
        rect.addEventListener("click", function () {
          if (state.tour) api.go(state.tour.index);
        });
        rect.addEventListener("keydown", function (event) {
          if (event.key === "Enter" && state.tour) api.go(state.tour.index);
        });
        state.tourRect = rect;
      }
      svgRoot.appendChild(state.tourRect);
      state.tour = { index: -1, phase: "pause", from: null, to: null, phaseStart: 0 };
      wake();
    }

    function stopTour() {
      if (state.tourRect && state.tourRect.parentNode) {
        state.tourRect.parentNode.removeChild(state.tourRect);
      }
      state.tour = null;
    }

    function easeInOut(t) {
      return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
    }

    function tourStep(now) {
      var tour = state.tour;
      if (!tour) return;
      if (!tour.phaseStart) tour.phaseStart = now;
      var discrete = reducedMotion();
      var elapsed = now - tour.phaseStart;
      if (tour.phase === "pause" && elapsed >= TOUR_PAUSE_MS) {
        tour.index = (tour.index + 1) % manifest.slides.length;
        tour.from = tour.to || slideWindow(tour.index);
        tour.to = slideWindow(tour.index);
        tour.phase = "move";
        tour.phaseStart = now;
        // Dim state follows the toured slide, no stagger.
        highlightTouredSlide(tour.index);
        // Morph follows too: the graph rearranges around each toured window.
        if (morphEnabled) {
          var win = slideWindow(tour.index);
          pushCtx = {
            cx: win.x + win.w / 2,
            cy: win.y + win.h / 2,
            rw: win.w / 2,
            rh: win.h / 2,
            margin: PUSH_MARGIN,
          };
        }
      } else if (tour.phase === "move" && (elapsed >= TOUR_MOVE_MS || discrete)) {
        tour.phase = "pause";
        tour.phaseStart = now;
      }
      var current;
      if (tour.phase === "move" && tour.from && !discrete) {
        var k = easeInOut(clamp(elapsed / TOUR_MOVE_MS, 0, 1));
        current = {
          x: tour.from.x + (tour.to.x - tour.from.x) * k,
          y: tour.from.y + (tour.to.y - tour.from.y) * k,
          w: tour.from.w + (tour.to.w - tour.from.w) * k,
          h: tour.from.h + (tour.to.h - tour.from.h) * k,
        };
      } else {
        current = tour.to;
      }
      if (current) {
        state.tourRect.setAttribute("x", current.x);
        state.tourRect.setAttribute("y", current.y);
        state.tourRect.setAttribute("width", Math.max(current.w, 1));
        state.tourRect.setAttribute("height", Math.max(current.h, 1));
      }
    }

    function highlightTouredSlide(index) {
      var slide = manifest.slides[index];
      var member = Object.create(null);
      pushMembers = Object.create(null);
      slide.nodes.forEach(function (node) {
        member[node.elementId] = true;
        if (morphNodes[node.elementId]) pushMembers[node.elementId] = true;
      });
      (slide.edges || []).forEach(function (edge) {
        member[edge.elementId] = true;
      });
      (slide.clusters || []).forEach(function (cluster) {
        member[cluster.elementId] = true;
      });
      Object.keys(elements).forEach(function (id) {
        classify(elements[id], "fm-deck-dim", !member[id]);
        classify(elements[id], "fm-deck-half", false);
        classify(elements[id], "fm-deck-hidden", false);
      });
    }

    /* ── Render loop (parks when idle) ───────────────────────────── */

    function morphFrame(now) {
      var n, i;
      for (i = 0; i < morphNodeIds.length; i += 1) {
        n = morphNodes[morphNodeIds[i]];
        var push = pushTargetOf(n);
        n.px += (push[0] - n.px) * PUSH_LERP;
        n.py += (push[1] - n.py) * PUSH_LERP;
        if (!n.dragging) {
          n.tx *= DRAG_RETURN;
          n.ty *= DRAG_RETURN;
        }
        var bobX = Math.sin(now * FLOAT_SPEED_X + n.phase) * FLOAT_AMP;
        var bobY = Math.cos(now * FLOAT_SPEED_Y + n.phase * 1.4) * FLOAT_AMP;
        n.fx = n.hx + bobX + n.px + n.tx;
        n.fy = n.hy + bobY + n.py + n.ty;
        n.el.setAttribute(
          "transform",
          "translate(" + (n.fx - n.hx).toFixed(2) + " " + (n.fy - n.hy).toFixed(2) + ")"
        );
      }
      for (i = 0; i < liveEdges.length; i += 1) {
        var edge = liveEdges[i];
        var a = borderPoint(edge.from, edge.to.fx, edge.to.fy);
        var b = borderPoint(edge.to, edge.from.fx, edge.from.fy);
        edge.live.setAttribute(
          "d",
          "M " + a[0].toFixed(2) + " " + a[1].toFixed(2) + " L " + b[0].toFixed(2) + " " + b[1].toFixed(2)
        );
        // The label (still inside the parked engine group) rides to the live midpoint.
        var mx = (a[0] + b[0]) / 2;
        var my = (a[1] + b[1]) / 2;
        edge.group.setAttribute(
          "transform",
          "translate(" + (mx - edge.mx0).toFixed(2) + " " + (my - edge.my0).toFixed(2) + ")"
        );
      }
    }

    function frame(now) {
      state.raf = 0;
      if (state.destroyed) return;
      var busy = false;
      if (state.tour) {
        tourStep(now);
        busy = true;
      }
      if (morphEnabled && stageOnScreen) {
        morphFrame(now);
        busy = true; // the world floats for as long as it is watched
      }
      if (state.target && !state.freeCam) {
        var dx = state.target.x - state.cam.x;
        var dy = state.target.y - state.cam.y;
        var ds = state.target.s - state.cam.s;
        var pixelError =
          Math.abs(dx) * state.cam.s + Math.abs(dy) * state.cam.s + Math.abs(ds) * 800;
        if (pixelError < SNAP_EPSILON) {
          state.cam = { x: state.target.x, y: state.target.y, s: state.target.s };
          state.target = null;
        } else {
          state.cam.x += dx * CAMERA_LERP;
          state.cam.y += dy * CAMERA_LERP;
          state.cam.s += ds * CAMERA_LERP;
          busy = true;
        }
        applyCamera();
      }
      if (busy) {
        state.raf = requestAnimationFrame(frame);
      } else {
        state.parked = true; // the loop PARKS — a background section must not burn frames
      }
    }

    function wake() {
      if (state.destroyed || state.raf) return;
      state.parked = false;
      state.raf = requestAnimationFrame(frame);
    }

    /* ── Navigation ──────────────────────────────────────────────── */

    function enterScene(index, stagger) {
      state.scene = ((index % sceneCount) + sceneCount) % sceneCount;
      state.step = 0; // EVERY entry route lands at step 0
      state.freeCam = false;
      hideTip();
      if (isOverview(state.scene)) {
        applyScene(false);
        startTour();
      } else {
        stopTour();
        applyScene(stagger);
      }
      retarget();
      onSlideChange(state.scene, currentSlide());
      noteInteraction();
    }

    var api = {
      next: function () {
        if (state.step < maxStepOf(state.scene)) {
          state.step += 1;
          applyScene(true);
          retarget();
          noteInteraction();
        } else {
          enterScene(state.scene + 1, false);
        }
      },
      prev: function () {
        // graphcon back(): un-reveal first; only at step 0 change slide (landing at ITS
        // step 0 — slide entry always resets).
        if (state.step > 0) {
          state.step -= 1;
          applyScene(false);
          retarget();
          noteInteraction();
        } else {
          enterScene(state.scene - 1, false);
        }
      },
      go: function (index) {
        enterScene(index, false);
      },
      overview: function () {
        if (overviewEnabled) enterScene(manifest.slides.length, false);
      },
      exitFreeCam: function () {
        state.freeCam = false;
        retarget();
      },
      destroy: destroy,
    };

    if (ui.nextBtn) ui.nextBtn.addEventListener("click", api.next);
    if (ui.prevBtn) ui.prevBtn.addEventListener("click", api.prev);
    if (ui.overviewBtn) ui.overviewBtn.addEventListener("click", api.overview);

    /* ── Free camera: drag pan, wheel zoom about cursor, pinch ───── */

    var pointers = new Map();
    var panState = null;
    var pinchState = null;
    var nodeDrag = null; // {node, startX, startY, otx, oty, moved, downTarget}

    function screenToWorld(clientX, clientY) {
      var rect = stage.getBoundingClientRect();
      var size = stageSize();
      return {
        x: state.cam.x + (clientX - rect.left - size.w / 2) / state.cam.s,
        y: state.cam.y + (clientY - rect.top - size.h / 2) / state.cam.s,
      };
    }

    function onPointerDown(event) {
      hideTip();
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (pointers.size === 1 && morphEnabled) {
        // Grab a home-team node instead of panning: it follows the pointer and springs
        // back on release (graphcon view-mode drag). Exiles still click-to-travel.
        var grabId = elementIdAt(event.target);
        var grab = grabId && morphNodes[grabId];
        if (grab && !grab.el.classList.contains("fm-deck-dim")) {
          nodeDrag = {
            node: grab,
            startX: event.clientX,
            startY: event.clientY,
            otx: grab.tx,
            oty: grab.ty,
            moved: false,
            downTarget: event.target,
          };
          grab.dragging = true;
          try {
            stage.setPointerCapture(event.pointerId);
          } catch (_error) {
            /* best-effort */
          }
          return;
        }
      }
      if (pointers.size === 1) {
        panState = {
          startX: event.clientX,
          startY: event.clientY,
          camX: state.cam.x,
          camY: state.cam.y,
          moved: false,
          // Pointer capture (below) retargets the matching pointerup to the STAGE, so the
          // original press target must be remembered here for click resolution.
          downTarget: event.target,
        };
      } else if (pointers.size === 2) {
        panState = null;
        if (nodeDrag) {
          nodeDrag.node.dragging = false;
          nodeDrag = null; // a second finger means pinch, not drag
        }
        var pair = Array.from(pointers.values());
        var mid = { x: (pair[0].x + pair[1].x) / 2, y: (pair[0].y + pair[1].y) / 2 };
        pinchState = {
          distance: Math.hypot(pair[0].x - pair[1].x, pair[0].y - pair[1].y) || 1,
          scale: state.cam.s,
          world: screenToWorld(mid.x, mid.y),
        };
      }
      try {
        stage.setPointerCapture(event.pointerId);
      } catch (_error) {
        /* pointer capture is best-effort */
      }
    }

    function onPointerMove(event) {
      if (!pointers.has(event.pointerId)) return;
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (nodeDrag) {
        var ndx = (event.clientX - nodeDrag.startX) / state.cam.s;
        var ndy = (event.clientY - nodeDrag.startY) / state.cam.s;
        if (Math.abs(ndx) + Math.abs(ndy) > 4 / state.cam.s) nodeDrag.moved = true;
        if (nodeDrag.moved) {
          nodeDrag.node.tx = nodeDrag.otx + ndx;
          nodeDrag.node.ty = nodeDrag.oty + ndy;
        }
        return;
      }
      if (pinchState && pointers.size >= 2) {
        state.freeCam = true;
        var pair = Array.from(pointers.values());
        var mid = { x: (pair[0].x + pair[1].x) / 2, y: (pair[0].y + pair[1].y) / 2 };
        var distance = Math.hypot(pair[0].x - pair[1].x, pair[0].y - pair[1].y) || 1;
        state.cam.s = clamp((pinchState.scale * distance) / pinchState.distance, 0.05, 6);
        var rect = stage.getBoundingClientRect();
        var size = stageSize();
        state.cam.x = pinchState.world.x - (mid.x - rect.left - size.w / 2) / state.cam.s;
        state.cam.y = pinchState.world.y - (mid.y - rect.top - size.h / 2) / state.cam.s;
        applyCamera();
        noteInteraction();
        return;
      }
      if (!panState) return;
      var dx = event.clientX - panState.startX;
      var dy = event.clientY - panState.startY;
      if (Math.abs(dx) + Math.abs(dy) > 3) {
        panState.moved = true;
        state.freeCam = true;
      }
      if (state.freeCam) {
        state.cam.x = panState.camX - dx / state.cam.s;
        state.cam.y = panState.camY - dy / state.cam.s;
        applyCamera();
        noteInteraction();
      }
    }

    function onPointerUp(event) {
      var clickTarget = panState && !panState.moved ? panState.downTarget : null;
      if (nodeDrag) {
        if (!nodeDrag.moved) clickTarget = nodeDrag.downTarget;
        nodeDrag.node.dragging = false; // spring-back takes over in the loop
        nodeDrag = null;
      }
      pointers.delete(event.pointerId);
      if (pointers.size < 2) pinchState = null;
      if (pointers.size === 0) panState = null;
      if (clickTarget) handleClick(clickTarget);
    }

    function onWheel(event) {
      event.preventDefault();
      state.freeCam = true;
      var delta = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
      var factor = Math.exp(-clamp(delta, -30, 30) * (event.ctrlKey ? 0.0015 : 0.0006));
      var world = screenToWorld(event.clientX, event.clientY);
      state.cam.s = clamp(state.cam.s * factor, 0.05, 6);
      var rect = stage.getBoundingClientRect();
      var size = stageSize();
      state.cam.x = world.x - (event.clientX - rect.left - size.w / 2) / state.cam.s;
      state.cam.y = world.y - (event.clientY - rect.top - size.h / 2) / state.cam.s;
      applyCamera();
      noteInteraction();
    }

    function onDoubleClick() {
      state.freeCam = false;
      retarget();
    }

    /* ── Travel + tooltips ───────────────────────────────────────── */

    function elementIdAt(eventTarget) {
      var el = eventTarget;
      while (el && el !== svgRoot) {
        if (el.id && elements[el.id]) return el.id;
        el = el.parentNode;
      }
      return null;
    }

    function handleClick(pressTarget) {
      var id = elementIdAt(pressTarget);
      if (!id) return;
      var el = elements[id];
      if (el.classList.contains("fm-deck-dim")) {
        var owners = slideIndexOfId[id];
        if (owners && owners.length) {
          for (var s = 0; s < manifest.slides.length; s += 1) {
            if (manifest.slides[s].id === owners[0]) {
              api.go(s);
              return;
            }
          }
        }
        return;
      }
      if (tooltipOf[id]) {
        if (tip.classList.contains("fm-deck-tip-show") && tip._for === id) hideTip();
        else showTip(id);
      }
    }

    function showTip(id) {
      tip.textContent = tooltipOf[id]; // untrusted deck text: textContent ONLY
      tip._for = id;
      tip.classList.add("fm-deck-tip-show");
      var elRect = elements[id].getBoundingClientRect();
      var stageRect = stage.getBoundingClientRect();
      tip.style.left = "0px";
      tip.style.top = "0px";
      var width = tip.offsetWidth;
      var height = tip.offsetHeight;
      var x = elRect.left - stageRect.left + elRect.width / 2 - width / 2;
      var y = elRect.top - stageRect.top - height - 10;
      x = clamp(x, 8, Math.max(8, stage.clientWidth - width - 8));
      if (y < 8) y = elRect.bottom - stageRect.top + 10; // flip below when clipped at top
      tip.style.left = x + "px";
      tip.style.top = y + "px";
    }

    function hideTip() {
      tip._for = null;
      tip.classList.remove("fm-deck-tip-show");
    }

    /* ── Keyboard (STAGE-SCOPED, never window) ───────────────────── */
    // The host gives the stage tabindex; stopPropagation keeps the showcase's global
    // spotlight arrow-key handler from double-firing.

    function onKeyDown(event) {
      var handled = true;
      if (event.key === "ArrowRight" || event.key === " ") api.next();
      else if (event.key === "ArrowLeft") api.prev();
      else if (event.key === "o" || event.key === "O") api.overview();
      else if (event.key === "f" || event.key === "F") {
        if (stage.requestFullscreen) stage.requestFullscreen();
      } else if (event.key === "Escape") api.exitFreeCam();
      else if (event.key === "Home") api.go(0);
      else if (event.key === "End") api.go(sceneCount - 1);
      else handled = false;
      if (handled) {
        event.preventDefault();
        event.stopPropagation();
      }
    }

    /* ── Autoplay (kiosk) ────────────────────────────────────────── */

    var autoAdvanceMs = manifest.options.autoAdvanceMs || 0;

    function noteInteraction() {
      if (autoAdvanceMs > 0) {
        state.autoplayPausedUntil = Date.now() + autoAdvanceMs * 2;
      }
    }

    function startAutoplay() {
      if (autoAdvanceMs <= 0 || reducedMotion()) return;
      // A pending timer counts as activity only when it FIRES; the rAF loop still parks
      // between ticks.
      state.autoplayTimer = setInterval(function () {
        if (Date.now() < state.autoplayPausedUntil) return;
        if (state.step < maxStepOf(state.scene)) {
          state.step += 1;
          applyScene(true);
          retarget();
        } else {
          enterScene(state.scene + 1, false);
        }
      }, autoAdvanceMs);
    }

    /* ── Resize ──────────────────────────────────────────────────── */

    var resizeObserver = null;
    if (typeof ResizeObserver === "function") {
      resizeObserver = new ResizeObserver(function () {
        if (!state.freeCam) retarget();
        else applyCamera();
      });
      resizeObserver.observe(stage);
    }

    // Morph's continuous loop runs ONLY while the stage is actually on screen — a deck
    // parked further down a page must not burn frames.
    var visibilityObserver = null;
    if (morphEnabled && typeof IntersectionObserver === "function") {
      visibilityObserver = new IntersectionObserver(function (entries) {
        for (var v = 0; v < entries.length; v += 1) {
          stageOnScreen = entries[v].isIntersecting;
        }
        if (stageOnScreen) wake();
      });
      visibilityObserver.observe(stage);
    }

    /* ── Teardown ────────────────────────────────────────────────── */

    function destroy() {
      if (state.destroyed) return;
      state.destroyed = true;
      clearStagger();
      stopTour();
      if (state.autoplayTimer) clearInterval(state.autoplayTimer);
      if (state.raf) cancelAnimationFrame(state.raf);
      if (resizeObserver) resizeObserver.disconnect();
      if (visibilityObserver) visibilityObserver.disconnect();
      stage.removeEventListener("pointerdown", onPointerDown);
      stage.removeEventListener("pointermove", onPointerMove);
      stage.removeEventListener("pointerup", onPointerUp);
      stage.removeEventListener("pointercancel", onPointerUp);
      stage.removeEventListener("wheel", onWheel);
      stage.removeEventListener("dblclick", onDoubleClick);
      stage.removeEventListener("keydown", onKeyDown);
      if (ui.nextBtn) ui.nextBtn.removeEventListener("click", api.next);
      if (ui.prevBtn) ui.prevBtn.removeEventListener("click", api.prev);
      if (ui.overviewBtn) ui.overviewBtn.removeEventListener("click", api.overview);
      if (viewport.parentNode) viewport.parentNode.removeChild(viewport);
      if (style.parentNode) style.parentNode.removeChild(style);
      if (tip.parentNode) tip.parentNode.removeChild(tip);
      if (liveRegion.parentNode) liveRegion.parentNode.removeChild(liveRegion);
    }

    /* ── Wire up and start ───────────────────────────────────────── */

    stage.addEventListener("pointerdown", onPointerDown);
    stage.addEventListener("pointermove", onPointerMove);
    stage.addEventListener("pointerup", onPointerUp);
    stage.addEventListener("pointercancel", onPointerUp);
    stage.addEventListener("wheel", onWheel, { passive: false });
    stage.addEventListener("dblclick", onDoubleClick);
    stage.addEventListener("keydown", onKeyDown);

    enterScene(0, false);
    // Land instantly on first paint rather than tweening in from nowhere.
    if (state.target) {
      state.cam = { x: state.target.x, y: state.target.y, s: state.target.s };
      state.target = null;
      applyCamera();
    }
    startAutoplay();

    return api;
  }

  var runtime = { mount: mount };
  if (typeof window !== "undefined") window.FmDeckRuntime = runtime;
})();
