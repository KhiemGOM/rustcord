// egui-wgpu's egui.wgsl for a gamma-space framebuffer, in HLSL.

cbuffer Locals : register(b0) {
    float2 screen_size;
    uint dithering;
    uint pad;
};

struct VsIn {
    float2 pos : POSITION;
    float2 uv : TEXCOORD;
    float4 color : COLOR;
};

struct PsIn {
    float4 pos : SV_POSITION;
    float2 uv : TEXCOORD;
    float4 color : COLOR;
};

PsIn vs_main(VsIn i) {
    PsIn o;
    o.pos = float4(2.0 * i.pos.x / screen_size.x - 1.0, 1.0 - 2.0 * i.pos.y / screen_size.y, 0.0, 1.0);
    o.uv = i.uv;
    o.color = i.color;
    return o;
}

Texture2D tex : register(t0);
SamplerState samp : register(s0);

float interleaved_gradient_noise(float2 n) {
    float f = 0.06711056 * n.x + 0.00583715 * n.y;
    return frac(52.9829189 * frac(f));
}

float4 dithered(float4 c, float2 at) {
    if (dithering == 1) {
        float noise = (interleaved_gradient_noise(at) - 0.5) * 0.95;
        c.rgb = c.rgb + noise / 255.0;
    }
    return c;
}

float4 ps_main(PsIn i) : SV_Target {
    return dithered(i.color * tex.Sample(samp, i.uv), i.pos.xy);
}

// the glyph atlas kept as one channel: white at that coverage
float4 ps_font(PsIn i) : SV_Target {
    return dithered(i.color * tex.Sample(samp, i.uv).r, i.pos.xy);
}

// video surfaces whose alpha byte is undefined (BGRX / VideoProcessor output)
float4 ps_opaque(PsIn i) : SV_Target {
    float4 c = tex.Sample(samp, i.uv);
    return dithered(i.color * float4(c.rgb, 1.0), i.pos.xy);
}
