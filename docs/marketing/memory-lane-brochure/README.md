# Memory Lane brochure

Two-page US Letter handout for families and care-community administrators.
Print the PDF at actual size, optionally double-sided with long-edge binding.
The PDF includes a clickable demonstration contact:
sam@advanceddatamachines.com. No pricing or clinical outcomes are promised.

Files:

- `Memory-Lane-Brochure.pdf`: shareable/printable brochure.
- `brochure.html`: editable layout and copy, with local assets only.
- `preview-page-1.png` and `preview-page-2.png`: page previews.
- `assets/`: hardware photographs, actual application screenshots, and an
  illustrative family photo.

Re-render with Chrome and Puppeteer available:

```sh
PUPPETEER_MODULE=/path/to/puppeteer/lib/puppeteer/puppeteer.js node render.mjs
```

The renderer checks image loading and page bounds, writes the PDF, and
captures both page previews. No production service or user account is used.

## Additional family versions

Two additional PDFs use the same layout, actual hardware/app images, copy
and contact details, with different illustrative family photographs:

- `Memory-Lane-Brochure-Black-Family.pdf`
- `Memory-Lane-Brochure-Indian-Family.pdf`

Their generated assets are `assets/family-photo-black.png` and
`assets/family-photo-indian.png`. Both show fictional grandmothers and
grandchildren and retain the brochure's AI-generated example label. The
original brochure and family photograph are preserved.

Regenerate both versions with:

```sh
PUPPETEER_MODULE=/path/to/puppeteer/lib/puppeteer/puppeteer.js node render.mjs black indian
```

The renderer substitutes only the TV photograph and its alternative text.
Previews use the prefixes `preview-black-family` and `preview-indian-family`.
Exact final prompts and the built-in generation method are recorded in
`variant-image-prompts.json`.

## The Dude edition

`Memory-Lane-Brochure-The-Dude.pdf` is an unofficial humorous concept featuring
an original AI-generated illustration of the Dude from The Big Lebowski on
the TV. It changes the opening sentence to "A slideshow that really ties the
room together" and labels the image as an unofficial character illustration.
Hardware, app screenshots, contact and product information are retained.
The asset is `assets/the-dude.png`; its final prompt and generation method
are in `the-dude-image-prompt.json`. Render with `node render.mjs dude` using
the Puppeteer environment setting above.

`Memory-Lane-Brochure-The-Dude-Family.pdf` adds his young red-haired daughter,
with a family resemblance to Julianne Moore, beside him on the sofa. Render
with `node render.mjs dude-family`. The image is a reference-based edit of
the original generated character illustration, saved separately at
`assets/the-dude-family.png`; the exact built-in-tool edit prompt is recorded
in `the-dude-family-image-prompt.json`. Both Dude editions are retained.

## Image sources

- `aluminium-case.png`: copy of the user-supplied
  `docs/device-images/Screenshot From 2026-11-15 08-17-46.png`.
- `case-profile.png`: copy of the user-supplied
  `docs/device-images/Screenshot From 2026-11-15 08-18-30.png`, retained as
  an alternate layout asset. Original hardware images are unchanged. The
  case is the existing prototype enclosure, not custom Memory Lane hardware.
- `app-home.png`: actual application capture from
  `/tmp/memory-lane-owner-navigation.png`, using example frame names.
- `app-send.png`: actual application capture from
  `/tmp/memory-lane-photo-picker.png`, using example frame names.
- `family-photo.png`: generated using the built-in image-generation tool;
  fictional people, not a customer testimonial. It is labelled as an
  AI-generated example in the brochure. The TV surround is HTML/CSS.

Final image prompt, built-in tool mode:

> Create one warm, candid, realistic family snapshot for a digital photo-frame brochure. Wide landscape 16:9 composition: a smiling grandmother in her late 60s and her young grandchild enjoying a sunny garden picnic together, a genuine affectionate moment, natural faces and hands, no one looking like a stock model. Soft golden afternoon light, leafy green garden, warm neutral clothing, tasteful understated colors, documentary family photography. The photo will be shown inside a TV screen in a separately designed brochure. Output ONLY the photograph, edge to edge; no television, no room mockup, no device, no text, no logo, no watermark. Fictional people; illustrative marketing image.

## Copy boundaries

The brochure describes current web-app access rather than suggesting a
released iOS/Android app. It qualifies TV compatibility with HDMI and states
power/Wi-Fi/input requirements. Offline playback refers to previously
downloaded photos. The pilot is an invitation to discuss a trial, without
unapproved pricing, free support commitments or guaranteed outcomes.
