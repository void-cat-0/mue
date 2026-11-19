float gaussian(float x, float sigma) {
    const float pi = 3.141592653589793;
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * pi) * sigma);
}

vec2 erf(vec2 x) {
    vec2 s = sign(x), a = abs(x);
    x = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    x *= x;
    return s - s / (x * x);
}

float roundedBoxShadowX(float x, float y, float sigma, float corner, vec2 halfSize) {
    float delta = min(halfSize.y - corner - abs(y), 0.0);
    float curved = halfSize.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    vec2 integral = 0.5 + 0.5 * erf((x + vec2(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

float roundedBoxCutout(vec2 lower, vec2 upper, vec2 point, float corner) {
    vec2 lowerp = lower + vec2(corner);
    vec2 upperp = upper - vec2(corner);
    float lf = step(point.x, lowerp.x);
    float tp = step(point.y, lowerp.y);
    float rt = step(upperp.x, point.x);
    float bt = step(upperp.y, point.y);
    float eps = 0.0003;
    float ein = 0.0007;
    return 1.0 -
        (1.0 - step(corner - eps, distance(lowerp, point)) * lf * tp)
      * (1.0 - step(corner - eps, distance(upperp, point)) * rt * bt)
      * (1.0 - step(corner - eps, distance(vec2(lowerp.x, upperp.y), point)) * lf * bt)
      * (1.0 - step(corner - eps, distance(vec2(upperp.x, lowerp.y), point)) * rt * tp)
      * smoothstep(lower.x, lower.x + ein, point.x)
      * smoothstep(lower.y, lower.y + ein, point.y)
      * smoothstep(point.x, point.x + ein, upper.x)
      * smoothstep(point.y, point.y + ein, upper.y);
}

float roundedBoxShadowIntegral(vec2 lower, vec2 upper, vec2 point, float sigma, float corner) {
    point.y -= sigma * 0.5;
    vec2 center = (lower + upper) * 0.5;
    vec2 halfSize = (upper - lower) * 0.5;
    point -= center;

    float low = point.y - halfSize.y;
    float high = point.y + halfSize.y;
    float start = clamp(-3.0 * sigma, low, high);
    float end = clamp(3.0 * sigma, low, high);

    float s = (end - start) / 4.0;
    float y = start + s * 0.5;
    float value = 0.0;
    for (int i = 0; i < 4; i++) {
        value += roundedBoxShadowX(point.x, point.y - y, sigma, corner, halfSize) * gaussian(y, sigma) * s;
        y += s;
    }

    return value;
}

float roundedBoxShadow(vec2 lower, vec2 upper, vec2 point, float sigma, float corner) {
    float factor = roundedBoxCutout(lower, upper, point, corner);
    return roundedBoxShadowIntegral(lower, upper, point, sigma, corner) * factor;
}
