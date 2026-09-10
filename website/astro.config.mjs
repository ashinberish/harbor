// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Project (not user/org) GitHub Pages site: served at
// https://ashinberish.github.io/harbor/, so `base` must match the repo name.
export default defineConfig({
  site: 'https://ashinberish.github.io',
  base: '/harbor',
  integrations: [
    starlight({
      title: 'Harbor',
      description:
        'A cross-platform, language-agnostic application hosting platform: process supervision, reverse proxy, and SSL behind one daemon, CLI, and GUI.',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/ashinberish/harbor' },
      ],
      editLink: {
        baseUrl: 'https://github.com/ashinberish/harbor/edit/cl/lucid-babbage-3kdpkl/website/',
      },
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Overview', slug: 'index' },
            { label: 'Getting Started', slug: 'getting-started' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'CLI Reference', slug: 'cli-reference' },
            { label: 'Configuration', slug: 'configuration' },
            { label: 'Architecture', slug: 'architecture' },
          ],
        },
        {
          label: 'Project',
          items: [{ label: 'Roadmap & Status', slug: 'roadmap' }],
        },
      ],
    }),
  ],
});
