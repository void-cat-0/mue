<style>
.mue-demo {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    align-items: stretch;
    gap: 0.75rem;
    margin: 1.25rem 0;
}
.mue-demo-code { min-width: 0; }
.mue-demo-code > :is(.example-wrap, pre, .code-block-wrap) { margin: 0; }
.mue-demo-code .rust { margin: 0; }
.mue-demo-preview {
    position: relative;
    overflow: hidden;
    min-width: 0;
    margin: 0;
    border-radius: 0.5rem;
    background: #ecf3ee;
}
.mue-demo-preview.mue-playback { background: #eaf0fc; }
.mue-demo-preview.mue-style-preview { background: #ebf2fa; }
.mue-demo-preview.mue-sky { background: #23364a; }
.mue-demo-preview > :is(p, .mue-motion, .mue-still) {
    position: absolute;
    inset: 0;
    margin: 0;
}
.mue-demo-preview > :is(.mue-motion, .mue-still) p { height: 100%; margin: 0; }
.mue-demo-preview img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
}
.mue-demo-preview.mue-preview-pair > p { height: 50%; }
.mue-demo-preview.mue-preview-pair > p + p { top: 50%; }
.mue-demo-preview .mue-still, .mue-chart .mue-still { display: none; }
.mue-chart { margin: 1.25rem 0; }
.mue-chart p { margin: 0; }
.mue-chart img { display: block; max-width: 100%; height: auto; border-radius: 0.5rem; }
.mue-chart figcaption { margin-top: 0.5rem; font-size: 0.92em; }
@media (max-width: 1050px) {
    .mue-demo { grid-template-columns: minmax(0, 1fr); }
    .mue-demo-preview > :is(p, .mue-motion, .mue-still) { position: static; }
    .mue-demo-preview img { height: auto; max-height: 24rem; }
}
@media (prefers-reduced-motion: reduce) {
    .mue-demo-preview .mue-motion, .mue-chart .mue-motion { display: none; }
    .mue-demo-preview .mue-still, .mue-chart .mue-still { display: block; }
}
</style>
