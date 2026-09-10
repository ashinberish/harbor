# Harbor docs

Built with [Astro](https://astro.build) + [Starlight](https://starlight.astro.build).
Deployed to GitHub Pages by `.github/workflows/deploy-docs.yml` on every push
that touches this directory.

```sh
npm install
npm run dev      # http://localhost:4321/harbor/
npm run build    # outputs to dist/
npm run preview  # serve the built site locally
```

Content lives in `src/content/docs/` as Markdown/MDX; sidebar structure and
site metadata are configured in `astro.config.mjs`.
