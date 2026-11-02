// Geanimeerde vortex als WebGL-shader (VX:W03.06): chroom met oranje vonken. Pauzeert als het
// venster verborgen of buiten beeld is; ruimt context en listeners op bij unmount.
import { useEffect, useRef } from "react";

const VERT = `attribute vec2 p; void main(){ gl_Position = vec4(p, 0.0, 1.0); }`;

const FRAG = `
precision highp float;
uniform vec2 r; uniform float t; uniform vec2 c; uniform float zoom;
float h(vec2 p){ vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
float n(vec2 p){ vec2 i=floor(p), f=fract(p); f=f*f*(3.0-2.0*f);
  return mix(mix(h(i),h(i+vec2(1,0)),f.x), mix(h(i+vec2(0,1)),h(i+vec2(1,1)),f.x), f.y); }
float fbm(vec2 p){ float v=0.0, a=0.5; for(int i=0;i<5;i++){ v+=a*n(p); p*=2.03; a*=0.5; } return v; }
void main(){
  vec2 uv = (gl_FragCoord.xy - c * r) / (r.y * zoom);
  float rad = length(uv);
  float ang = atan(uv.y, uv.x);
  float sw = ang * 2.0 + 2.6 / (rad + 0.12) - t * 0.55;
  // naadloos: ruis op (cos, sin) van de hoek i.p.v. de hoek zelf (atan springt bij ±pi)
  float b = fbm(vec2(cos(ang), sin(ang)) * 1.7 + vec2(rad * 5.0 - t * 0.25, 2.6 / (rad + 0.12) * 0.35 - t * 0.2));
  float ribbon = sin(sw * 2.0 + b * 5.0);
  float metal = smoothstep(-0.2, 0.9, ribbon);
  float spec = pow(max(0.0, sin(sw * 2.0 + b * 5.0 + 0.6)), 18.0);
  vec3 chrome = mix(vec3(0.03, 0.03, 0.035), vec3(0.78, 0.79, 0.82), metal) + vec3(spec);
  float groove = pow(1.0 - abs(ribbon), 6.0);
  vec3 orange = vec3(1.0, 0.36, 0.09) * groove * (0.55 + 0.6 * b) * exp(-rad * 1.1);
  float sp = step(0.9965, h(floor(gl_FragCoord.xy / 2.5) + floor(t * 4.0))) * smoothstep(1.4, 0.2, rad);
  vec3 col = chrome * exp(-rad * 0.9) + orange * 1.4 + vec3(1.0, 0.55, 0.25) * sp * 1.5;
  col *= smoothstep(0.015, 0.16, rad);
  col *= smoothstep(1.9, 0.35, rad);
  gl_FragColor = vec4(col, 1.0);
}`;

interface Renderer {
  draw: (w: number, h: number, t: number, center: [number, number], zoom: number) => void;
  dispose: () => void;
}

function createRenderer(canvas: HTMLCanvasElement, preserve: boolean): Renderer | null {
  const gl = canvas.getContext("webgl", { antialias: false, preserveDrawingBuffer: preserve });
  if (!gl) return null;
  const sh = (type: number, src: string) => {
    const s = gl.createShader(type);
    if (!s) return null;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  };
  const prog = gl.createProgram();
  const vs = sh(gl.VERTEX_SHADER, VERT);
  const fs = sh(gl.FRAGMENT_SHADER, FRAG);
  if (!prog || !vs || !fs) return null;
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  gl.useProgram(prog);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const loc = gl.getAttribLocation(prog, "p");
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  const uR = gl.getUniformLocation(prog, "r");
  const uT = gl.getUniformLocation(prog, "t");
  const uC = gl.getUniformLocation(prog, "c");
  const uZ = gl.getUniformLocation(prog, "zoom");
  return {
    draw(w, h, t, center, zoom) {
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      gl.viewport(0, 0, w, h);
      gl.uniform2f(uR, w, h);
      gl.uniform1f(uT, t);
      gl.uniform2f(uC, center[0], center[1]);
      gl.uniform1f(uZ, zoom);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    },
    dispose() {
      gl.deleteBuffer(buf);
      gl.deleteProgram(prog);
      gl.deleteShader(vs);
      gl.deleteShader(fs);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}

interface Props {
  /** Middelpunt als fractie van de breedte/hoogte. */
  center?: [number, number];
  zoom?: number;
  className?: string;
}

export function VortexShader({ center = [0.42, 0.5], zoom = 0.95, className }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  const cx = center[0];
  const cy = center[1];
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const r = createRenderer(canvas, false);
    if (!r) return;
    let raf = 0;
    let visible = true;
    const t0 = performance.now();
    const draw = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      r.draw(
        Math.max(1, Math.floor(canvas.clientWidth * dpr)),
        Math.max(1, Math.floor(canvas.clientHeight * dpr)),
        (performance.now() - t0) / 1000,
        [cx, cy],
        zoom,
      );
    };
    const loop = () => {
      draw();
      raf = visible && !document.hidden ? requestAnimationFrame(loop) : 0;
    };
    const resume = () => {
      if (visible && !document.hidden && !raf) raf = requestAnimationFrame(loop);
    };
    const io = new IntersectionObserver((es) => {
      visible = es.some((e) => e.isIntersecting);
      resume();
    });
    io.observe(canvas);
    document.addEventListener("visibilitychange", resume);
    loop();
    return () => {
      cancelAnimationFrame(raf);
      io.disconnect();
      document.removeEventListener("visibilitychange", resume);
      r.dispose();
    };
  }, [cx, cy, zoom]);
  return <canvas ref={ref} className={className} />;
}

const markCache = new Map<number, string>();

/** Eén frame als afbeelding (gecachet) — voor logo en avatars, zonder extra WebGL-contexten. */
function markUrl(px: number): string {
  const hit = markCache.get(px);
  if (hit !== undefined) return hit;
  const c = document.createElement("canvas");
  const r = createRenderer(c, true);
  let url = "";
  if (r) {
    r.draw(px, px, 7.0, [0.5, 0.5], 0.62);
    url = c.toDataURL("image/png");
    r.dispose();
  }
  markCache.set(px, url);
  return url;
}

export function VortexMark({ size }: { size: number }) {
  const px = Math.round(size * Math.min(window.devicePixelRatio || 1, 2));
  const src = markUrl(px);
  return src ? (
    <img src={src} width={size} height={size} alt="" style={{ display: "block", borderRadius: "50%" }} />
  ) : (
    <div style={{ width: size, height: size, borderRadius: "50%", background: "#333" }} />
  );
}
