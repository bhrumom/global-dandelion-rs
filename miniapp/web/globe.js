// Rendering adapters only. Geographic vertices and operation rings come from Rust/Wasm.
export function createGlobe(canvas, rust, status) {
  const gl = canvas.getContext('webgl', { alpha: true, antialias: true, preserveDrawingBuffer: true });
  if (!gl) return createSoftwareGlobe(canvas, rust, status);
  const vertex = `attribute vec3 position; attribute float land;
    uniform vec2 rotation; uniform float aspect; varying vec3 normal; varying vec3 geography; varying float terrain;
    void main(){float a=rotation.x,b=rotation.y;vec3 p=vec3(cos(a)*position.x+sin(a)*position.z,position.y,-sin(a)*position.x+cos(a)*position.z);
    p=vec3(p.x,cos(b)*p.y-sin(b)*p.z,sin(b)*p.y+cos(b)*p.z);normal=normalize(p);geography=normalize(position);terrain=land;
    float depth=3.5-p.z;gl_Position=vec4(p.x*2.65/aspect,p.y*2.65,1.0202*depth-.20202,depth);}`;
  const fragment = `precision mediump float;varying vec3 normal;varying vec3 geography;varying float terrain;uniform vec4 overlay;
    void main(){if(overlay.a>0.){gl_FragColor=overlay;return;}float light=.28+.72*max(0.,dot(normal,normalize(vec3(-.8,.7,1.2))));
    vec3 ocean=vec3(.018,.105,.15),land=vec3(.16,.48,.43);vec3 color=mix(ocean,land,smoothstep(.3,.65,terrain))*light;
    vec3 g=normalize(geography);float grid=1.-smoothstep(.0,.025,min(abs(sin(atan(g.x,g.z)*12.)),abs(sin(asin(clamp(g.y,-1.,1.))*12.))));color=mix(color,vec3(.12,.34,.3),grid*.28);
    float rim=pow(1.-max(0.,normal.z),3.);color+=rim*vec3(.03,.32,.38);gl_FragColor=vec4(color,1.);}`;
  const shaders = [], buffers = [], events = new AbortController();
  const compile = (type, source) => {
    const shader = gl.createShader(type); shaders.push(shader); gl.shaderSource(shader, source); gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error('3D shader compilation failed'); return shader;
  };
  let program;
  try {
    program = gl.createProgram(); gl.attachShader(program, compile(gl.VERTEX_SHADER, vertex)); gl.attachShader(program, compile(gl.FRAGMENT_SHADER, fragment)); gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error('3D program link failed');
  } catch { for (const shader of shaders) gl.deleteShader(shader); if (program) gl.deleteProgram(program); status('3D 初始化失败；请使用下方功能。'); return { update() {}, destroy() {} }; }
  gl.useProgram(program); gl.enable(gl.DEPTH_TEST); gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  const position = gl.getAttribLocation(program, 'position'), land = gl.getAttribLocation(program, 'land');
  const rotation = gl.getUniformLocation(program, 'rotation'), aspect = gl.getUniformLocation(program, 'aspect'), overlay = gl.getUniformLocation(program, 'overlay');
  const mesh = rust.globe_mesh(64, 128);
  const buffer = () => { const value = gl.createBuffer(); buffers.push(value); return value; };
  const surface = buffer(), lines = buffer(); gl.bindBuffer(gl.ARRAY_BUFFER, surface); gl.bufferData(gl.ARRAY_BUFFER, mesh, gl.STATIC_DRAW);
  let pulseUntil = 0;
  let yaw = -.5, pitch = .12, pointer = null, previous = 0, frame = 0, disposed = false;
  let operation = { active: false, mode: 'local', origin: null, destination: null, confirmed: false }, route = null;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const drawLine = (vertices, color) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, lines); gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position, 3, gl.FLOAT, false, 0, 0);
    gl.disableVertexAttribArray(land); gl.vertexAttrib1f(land, 0); gl.uniform4fv(overlay, color); gl.drawArrays(gl.LINE_STRIP, 0, vertices.length / 3);
  };
  function draw(time) {
    frame = 0;
    if (disposed || document.hidden || gl.isContextLost()) return;
    if (time - previous < 40) { frame = requestAnimationFrame(draw); return; }
    previous = time;
    const box = canvas.getBoundingClientRect(), ratio = Math.min(devicePixelRatio || 1, 1.75);
    const width = Math.max(1, Math.min(1400, Math.round(box.width * ratio))), height = Math.max(1, Math.min(1400, Math.round(box.height * ratio)));
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    gl.viewport(0, 0, width, height); gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.uniform2f(rotation, yaw, pitch); gl.uniform1f(aspect, width / height); gl.uniform4f(overlay, 0, 0, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, surface); gl.enableVertexAttribArray(position); gl.enableVertexAttribArray(land);
    gl.vertexAttribPointer(position, 3, gl.FLOAT, false, 16, 0); gl.vertexAttribPointer(land, 1, gl.FLOAT, false, 16, 12); gl.drawArrays(gl.TRIANGLES, 0, mesh.length / 4);
    if (route) drawLine(route, operation.confirmed ? [0.43, 1, .74, 1] : [.35, .75, 1, .65]);
    if ((operation.active || time < pulseUntil) && operation.origin && operation.mode !== 'global') {
      for (let i = 0; i < (reduced.matches ? 1 : 3); i++) {
        const phase = reduced.matches ? .5 : ((time / 2600 + i / 3) % 1);
        drawLine(rust.globe_ripple(...operation.origin, 1 + phase * 24, 64), operation.mode === 'hotspot' ? [1, .75, .35, 1 - phase] : [.35, .95, .85, 1 - phase]);
      }
    }
    if (route && operation.active && !reduced.matches) {
      const end = Math.max(2, Math.floor((time / 1600 % 1) * (route.length / 3)));
      drawLine(route.slice(Math.max(0, end - 6) * 3, end * 3), [.8, 1, 1, 1]);
    }
    if ((operation.active || time < pulseUntil) && !reduced.matches) frame = requestAnimationFrame(draw);
  }
  const redraw = () => { if (!frame && !disposed && !document.hidden) frame = requestAnimationFrame(draw); };
  canvas.addEventListener('pointerdown', event => { pointer = { x: event.clientX, y: event.clientY, id: event.pointerId }; canvas.setPointerCapture(event.pointerId); }, { signal: events.signal });
  canvas.addEventListener('pointermove', event => { if (!pointer || pointer.id !== event.pointerId) return; yaw += (event.clientX - pointer.x) * .008; pitch = Math.max(-1.3, Math.min(1.3, pitch + (event.clientY - pointer.y) * .008)); pointer.x = event.clientX; pointer.y = event.clientY; redraw(); }, { signal: events.signal });
  for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) canvas.addEventListener(name, () => { pointer = null; }, { signal: events.signal });
  canvas.addEventListener('keydown', event => { if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return; event.preventDefault(); yaw += event.key === 'ArrowLeft' ? -.12 : event.key === 'ArrowRight' ? .12 : 0; pitch = Math.max(-1.3, Math.min(1.3, pitch + (event.key === 'ArrowUp' ? -.12 : event.key === 'ArrowDown' ? .12 : 0))); redraw(); }, { signal: events.signal });
  document.addEventListener('visibilitychange', () => { if (document.hidden) { cancelAnimationFrame(frame); frame = 0; } else redraw(); }, { signal: events.signal });
  window.addEventListener('resize', redraw, { signal: events.signal });
  reduced.addEventListener('change', redraw, { signal: events.signal });
  canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); cancelAnimationFrame(frame); frame = 0; status('3D 画布已暂停；数据操作不受影响。'); }, { signal: events.signal });
  canvas.addEventListener('webglcontextrestored', () => { status('3D 画布需要重新打开小程序；请先停止当前操作。'); }, { signal: events.signal });
  redraw();
  return {
    update(next) {
      if (next.origin && (!operation.origin || next.origin.some((v, i) => v !== operation.origin[i]))) { yaw = -next.origin[1] * Math.PI / 180; pitch = next.origin[0] * Math.PI / 180; }
      if (next.confirmed === true) pulseUntil = performance.now() + 2400;
      if (next.confirmed === false) pulseUntil = 0;
      operation = { ...operation, ...next }; route = operation.mode === 'global' && (operation.active || operation.confirmed) && operation.origin && operation.destination ? rust.globe_route(...operation.origin, ...operation.destination, 96) : null;
      canvas.dataset.operation = operation.active ? 'running' : operation.confirmed ? 'confirmed' : 'idle'; redraw();
    },
    destroy() { disposed = true; cancelAnimationFrame(frame); events.abort(); for (const value of buffers) gl.deleteBuffer(value); for (const shader of shaders) gl.deleteShader(shader); gl.deleteProgram(program); },
  };
}

// Orthographic software projection keeps the Rust 3D model usable when Host
// policy disables WebGL. No extra Host capability or remote asset is required.
function createSoftwareGlobe(canvas, rust, status) {
  const ctx = canvas.getContext('2d');
  if (!ctx) { status('此设备未提供可用画布；数据操作仍可使用。'); return { update() {}, destroy() {} }; }
  canvas.dataset.renderer = 'software-3d';
  const mesh = rust.globe_mesh(32, 64), events = new AbortController();
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let yaw = -.5, pitch = .12, frame = 0, previous = -Infinity, disposed = false, pointer = null, pulseUntil = 0;
  let operation = { active: false, confirmed: false, mode: 'local', origin: null, destination: null }, route = null;
  const rotate = (x, y, z) => {
    const px = Math.cos(yaw) * x + Math.sin(yaw) * z, pz = -Math.sin(yaw) * x + Math.cos(yaw) * z;
    return [px, Math.cos(pitch) * y - Math.sin(pitch) * pz, Math.sin(pitch) * y + Math.cos(pitch) * pz];
  };
  const redraw = () => { if (!frame && !disposed && !document.hidden) frame = requestAnimationFrame(draw); };
  function draw(time) {
    frame = 0;
    if (disposed || document.hidden) return;
    if (time - previous < 1000 / 15) { frame = requestAnimationFrame(draw); return; }
    previous = time;
    const box = canvas.getBoundingClientRect(), ratio = Math.min(devicePixelRatio || 1, 1.5);
    const width = Math.max(1, Math.min(1000, Math.round(box.width * ratio))), height = Math.max(1, Math.min(1000, Math.round(box.height * ratio)));
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    const cx = width / 2, cy = height / 2, radius = Math.min(width, height) * .40;
    ctx.clearRect(0, 0, width, height);
    const ocean = ctx.createRadialGradient(cx - radius * .4, cy - radius * .4, radius * .05, cx, cy, radius);
    ocean.addColorStop(0, '#164e59'); ocean.addColorStop(.75, '#072b39'); ocean.addColorStop(1, '#061c25');
    ctx.beginPath(); ctx.arc(cx, cy, radius, 0, Math.PI * 2); ctx.fillStyle = ocean; ctx.fill();
    const faces = [];
    for (let offset = 0; offset < mesh.length; offset += 12) {
      const land = (mesh[offset + 3] + mesh[offset + 7] + mesh[offset + 11]) / 3;
      if (land < .3) continue;
      const a = rotate(...mesh.subarray(offset, offset + 3)), b = rotate(...mesh.subarray(offset + 4, offset + 7)), c = rotate(...mesh.subarray(offset + 8, offset + 11));
      const z = (a[2] + b[2] + c[2]) / 3;
      if (z <= 0) continue;
      faces.push({ a, b, c, z });
    }
    faces.sort((a, b) => a.z - b.z);
    ctx.save(); ctx.beginPath(); ctx.arc(cx, cy, radius, 0, Math.PI * 2); ctx.clip();
    for (const face of faces) {
      const { a, b, c, z } = face;
      const light = Math.max(.25, Math.min(1, .38 + .5 * z - .25 * (a[0] + b[0] + c[0]) / 3 + .2 * (a[1] + b[1] + c[1]) / 3));
      ctx.beginPath(); ctx.moveTo(cx + a[0] * radius, cy - a[1] * radius); ctx.lineTo(cx + b[0] * radius, cy - b[1] * radius); ctx.lineTo(cx + c[0] * radius, cy - c[1] * radius); ctx.closePath();
      ctx.fillStyle = `rgb(${Math.round(40 * light)},${Math.round(140 * light)},${Math.round(119 * light)})`;
      ctx.strokeStyle = ctx.fillStyle; ctx.lineWidth = .5; ctx.fill(); ctx.stroke();
    }
    ctx.restore();
    ctx.beginPath(); ctx.arc(cx, cy, radius, 0, Math.PI * 2); ctx.strokeStyle = '#3b897e66'; ctx.lineWidth = 1; ctx.stroke();
    const line = (vertices, color) => {
      ctx.beginPath(); let joined = false;
      for (let i = 0; i < vertices.length; i += 3) {
        const p = rotate(vertices[i], vertices[i + 1], vertices[i + 2]);
        if (p[2] <= 0) { joined = false; continue; }
        if (joined) ctx.lineTo(cx + p[0] * radius, cy - p[1] * radius);
        else ctx.moveTo(cx + p[0] * radius, cy - p[1] * radius);
        joined = true;
      }
      ctx.strokeStyle = color; ctx.lineWidth = 1.5 * ratio; ctx.stroke();
    };
    if (route) line(route, operation.confirmed ? '#71ffbe' : '#73bfff99');
    if (route && operation.active && !reduced.matches) {
      const end = Math.max(2, Math.floor((time / 1600 % 1) * route.length / 3));
      line(route.subarray(Math.max(0, end - 6) * 3, end * 3), '#e1fff8');
    }
    if ((operation.active || time < pulseUntil) && operation.origin && operation.mode !== 'global') {
      for (let i = 0; i < (reduced.matches ? 1 : 3); i++) {
        const phase = reduced.matches ? .5 : (time / 2600 + i / 3) % 1;
        line(rust.globe_ripple(...operation.origin, 1 + phase * 24, 64), operation.mode === 'hotspot' ? `rgba(255,192,90,${1 - phase})` : `rgba(90,245,210,${1 - phase})`);
      }
    }
    if ((operation.active || time < pulseUntil) && !reduced.matches) redraw();
  }
  canvas.addEventListener('pointerdown', event => { pointer = { x: event.clientX, y: event.clientY, id: event.pointerId }; canvas.setPointerCapture(event.pointerId); }, { signal: events.signal });
  canvas.addEventListener('pointermove', event => {
    if (!pointer || pointer.id !== event.pointerId) return;
    yaw += (event.clientX - pointer.x) * .008; pitch = Math.max(-1.3, Math.min(1.3, pitch + (event.clientY - pointer.y) * .008));
    pointer.x = event.clientX; pointer.y = event.clientY; redraw();
  }, { signal: events.signal });
  for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) canvas.addEventListener(name, () => { pointer = null; }, { signal: events.signal });
  canvas.addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault(); yaw += event.key === 'ArrowLeft' ? -.12 : event.key === 'ArrowRight' ? .12 : 0;
    pitch = Math.max(-1.3, Math.min(1.3, pitch + (event.key === 'ArrowUp' ? -.12 : event.key === 'ArrowDown' ? .12 : 0))); redraw();
  }, { signal: events.signal });
  document.addEventListener('visibilitychange', () => { if (document.hidden) { cancelAnimationFrame(frame); frame = 0; } else redraw(); }, { signal: events.signal });
  window.addEventListener('resize', redraw, { signal: events.signal }); reduced.addEventListener('change', redraw, { signal: events.signal });
  redraw();
  return {
    update(next) {
      if (next.origin && (!operation.origin || next.origin.some((v, i) => v !== operation.origin[i]))) { yaw = -next.origin[1] * Math.PI / 180; pitch = next.origin[0] * Math.PI / 180; }
      if (next.confirmed === true) pulseUntil = performance.now() + 2400;
      if (next.confirmed === false) pulseUntil = 0;
      operation = { ...operation, ...next };
      route = operation.mode === 'global' && (operation.active || operation.confirmed) && operation.origin && operation.destination ? rust.globe_route(...operation.origin, ...operation.destination, 96) : null;
      canvas.dataset.operation = operation.active ? 'running' : operation.confirmed ? 'confirmed' : 'idle'; redraw();
    },
    destroy() { disposed = true; cancelAnimationFrame(frame); events.abort(); },
  };
}
