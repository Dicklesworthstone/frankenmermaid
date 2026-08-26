/* frankenmermaid deck runtime (epic bd-z7g6k, bd-lnca7).
 *
 * Presents a rendered frankenmermaid SVG as a graphcon-deck-style slideshow, driven entirely
 * by the engine's DeckManifest: camera fit + tween per slide, dim/half focus choreography,
 * staggered step reveals, an overview scene with a window-replay tour, free camera, click-a-
 * dimmed-node travel, tooltips, autoplay, and keyboard control — with zero graph logic of its
 * own (membership, steps, ordering, and geometry are all precomputed in the manifest).
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

  function clamp(value, low, high) {
    return Math.min(high, Math.max(low, value));
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
    var slideIndexOfId = manifest.nodeSlideIndex || {};
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
    var elements = Object.create(null); // elementId -> Element
    var all = svgRoot.querySelectorAll("[id]");
    for (var i = 0; i < all.length; i += 1) {
      var candidate = all[i];
      if (wanted[candidate.id]) elements[candidate.id] = candidate;
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

    function sceneCameraTarget(sceneIndex) {
      if (isOverview(sceneIndex)) {
        return fitTarget(manifest.viewBox, manifest.options.fitMargin, manifest.options.zoomMax);
      }
      var slide = manifest.slides[sceneIndex];
      return fitTarget(slide.bounds, slide.fitMargin, slide.zoomMax);
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

    function retarget() {
      if (state.freeCam) return;
      state.target = sceneCameraTarget(state.scene);
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
          };
        });
      }
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
      slide.nodes.forEach(function (node) {
        member[node.elementId] = true;
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

    function frame(now) {
      state.raf = 0;
      if (state.destroyed) return;
      var busy = false;
      if (state.tour) {
        tourStep(now);
        busy = true;
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

    /* ── Teardown ────────────────────────────────────────────────── */

    function destroy() {
      if (state.destroyed) return;
      state.destroyed = true;
      clearStagger();
      stopTour();
      if (state.autoplayTimer) clearInterval(state.autoplayTimer);
      if (state.raf) cancelAnimationFrame(state.raf);
      if (resizeObserver) resizeObserver.disconnect();
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
