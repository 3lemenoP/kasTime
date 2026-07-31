// ESLint flat config (ESLint v9+).
//
// Goal: a WORKING, non-failing lint baseline for the current codebase.
// Noisy rules are intentionally set to "warn" (not "error") so that
// `npm run lint` exits 0 today. Tighten to "error" incrementally as the
// codebase is cleaned up.
//
// Scope: lints src/**/*.{ts,tsx} only. The generated WASM glue
// (src/wasm/**) and build output (dist/, site/) are ignored.

import js from '@eslint/js'
import tsParser from '@typescript-eslint/parser'
import tsPlugin from '@typescript-eslint/eslint-plugin'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'

export default [
  // Global ignores (must be a config object with ONLY `ignores`).
  {
    ignores: [
      'dist/**',
      'build/**',
      'site/**',
      'node_modules/**',
      'src/wasm/**', // generated wasm-bindgen glue; not hand-authored
      'scripts/**', // build/tooling scripts, not app source
      '**/*.config.js',
      '**/*.config.ts',
      'vite.config.ts',
    ],
  },

  // Base recommended JS rules.
  js.configs.recommended,

  // TypeScript / React source.
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: { jsx: true },
      },
      globals: {
        // Browser + a few common globals used by the app.
        window: 'readonly',
        document: 'readonly',
        navigator: 'readonly',
        console: 'readonly',
        fetch: 'readonly',
        WebSocket: 'readonly',
        localStorage: 'readonly',
        sessionStorage: 'readonly',
        setTimeout: 'readonly',
        clearTimeout: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        crypto: 'readonly',
        TextEncoder: 'readonly',
        TextDecoder: 'readonly',
        Blob: 'readonly',
        File: 'readonly',
        FileReader: 'readonly',
        URL: 'readonly',
        URLSearchParams: 'readonly',
        atob: 'readonly',
        btoa: 'readonly',
        alert: 'readonly',
        requestAnimationFrame: 'readonly',
        cancelAnimationFrame: 'readonly',
        HTMLElement: 'readonly',
        HTMLInputElement: 'readonly',
        HTMLDivElement: 'readonly',
        Event: 'readonly',
        CustomEvent: 'readonly',
        DragEvent: 'readonly',
        performance: 'readonly',
        structuredClone: 'readonly',
        process: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      // --- React hooks ---
      'react-hooks/rules-of-hooks': 'warn',
      'react-hooks/exhaustive-deps': 'warn', // intentionally warn, not error
      'react-refresh/only-export-components': [
        'warn',
        { allowConstantExport: true },
      ],

      // --- TypeScript: prefer the TS-aware unused-vars, disable the core one ---
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': [
        'warn',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/no-explicit-any': 'warn',

      // --- Core rules that would otherwise error on current code -> warn ---
      'no-empty': 'warn',
      'no-constant-condition': 'warn',
      'no-prototype-builtins': 'warn',
      'no-useless-escape': 'warn',
      'no-fallthrough': 'warn',
      'no-case-declarations': 'warn',

      // TS handles undefined-symbol checking; the core rule mis-fires on
      // type-only references, so disable it here.
      'no-undef': 'off',
    },
  },
]
