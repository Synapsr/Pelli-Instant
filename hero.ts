import { heroui } from '@heroui/react'

// Adapted from the theme of Pelli's web dashboard.
// Same charter: `primary` is the neutral grey scale (defined in full, HeroUI
// does not derive shades from DEFAULT), the violet accent #5840f0 stays a
// literal used through low-opacity gradients/borders.
//
// Pelli Instant specifics:
// - photos are judged on screen, so the `default` scale, backgrounds and
//   contents are *neutral* greys (no zinc/blue cast that would bias colours);
// - dark is the default theme: the primary button is light-on-dark there
//   (a #202020 button would vanish on the dark background);
// - focus rings use the violet accent so they stay visible in both themes;
// - verdict colours (success/warning/danger) are calm, Apple-like tones.

const neutralLight = {
  50: '#fafafa',
  100: '#f4f4f4',
  200: '#e6e6e6',
  300: '#d4d4d4',
  400: '#a3a3a3',
  500: '#737373',
  600: '#525252',
  700: '#404040',
  800: '#262626',
  900: '#171717',
}

const neutralDark = {
  50: '#181818',
  100: '#242424',
  200: '#353535',
  300: '#4a4a4a',
  400: '#6e6e6e',
  500: '#a0a0a0',
  600: '#cfcfcf',
  700: '#e3e3e3',
  800: '#f2f2f2',
  900: '#fafafa',
}

export default heroui({
  layout: {
    radius: {
      small: '6px',
      medium: '8px',
      large: '12px',
    },
    disabledOpacity: '0.45',
  },
  themes: {
    light: {
      colors: {
        background: '#ffffff',
        foreground: '#171717',
        divider: 'rgba(0, 0, 0, 0.08)',
        overlay: '#000000',
        focus: '#5840f0',
        content1: '#ffffff',
        content2: '#f6f6f6',
        content3: '#eeeeee',
        content4: '#e2e2e2',
        default: { ...neutralLight, DEFAULT: '#e6e6e6', foreground: '#171717' },
        primary: {
          50: '#f7f7f7',
          100: '#efefef',
          200: '#e0e0e0',
          300: '#c8c8c8',
          400: '#9a9a9a',
          500: '#6b6b6b',
          600: '#4d4d4d',
          700: '#333333',
          800: '#262626',
          900: '#171717',
          DEFAULT: '#202020',
          foreground: '#ffffff',
        },
        secondary: {
          DEFAULT: '#F2EAFA',
          foreground: '#5840f0',
        },
        success: { DEFAULT: '#1f9d4c', foreground: '#ffffff' },
        warning: { DEFAULT: '#c97a00', foreground: '#ffffff' },
        danger: { DEFAULT: '#e0362c', foreground: '#ffffff' },
      },
    },
    dark: {
      colors: {
        background: '#121212',
        foreground: '#ededed',
        divider: 'rgba(255, 255, 255, 0.08)',
        overlay: '#000000',
        focus: '#a599f5',
        content1: '#1a1a1a',
        content2: '#222222',
        content3: '#2b2b2b',
        content4: '#353535',
        default: { ...neutralDark, DEFAULT: '#353535', foreground: '#f2f2f2' },
        primary: {
          50: '#171717',
          100: '#262626',
          200: '#333333',
          300: '#4d4d4d',
          400: '#6b6b6b',
          500: '#9a9a9a',
          600: '#c8c8c8',
          700: '#e0e0e0',
          800: '#efefef',
          900: '#f7f7f7',
          DEFAULT: '#ededed',
          foreground: '#141414',
        },
        secondary: {
          DEFAULT: '#2a2440',
          foreground: '#a599f5',
        },
        success: { DEFAULT: '#34c759', foreground: '#0b1f10' },
        warning: { DEFAULT: '#ffb340', foreground: '#241500' },
        danger: { DEFAULT: '#ff5b4f', foreground: '#2a0705' },
      },
    },
  },
})
