import 'vuetify/styles'
import '@mdi/font/css/materialdesignicons.css'
import { createVuetify } from 'vuetify'
import * as components from 'vuetify/components'
import * as directives from 'vuetify/directives'
import { palette } from '@/theme/palette'

export default createVuetify({
  components,
  directives,
  theme: {
    defaultTheme: 'light',
    themes: {
      light: {
        dark: false,
        colors: {
          // Brand palette from logo.svg — single source in src/theme/palette.ts
          primary: palette.primary,
          secondary: palette.secondary,
          accent: palette.accent,
          error: palette.error,
          info: palette.info,
          success: palette.success,
          warning: palette.warning,
          background: palette.background,
          surface: palette.surface,
          'on-primary': palette.onPrimary,
          'on-secondary': palette.onSecondary,
          'on-surface': palette.onSurface,
          'on-background': palette.onBackground,
        },
      },
    },
  },
  defaults: {
    VBtn: {
      style: 'text-transform: none;',
    },
    VTextField: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VTextarea: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VSelect: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VAutocomplete: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VCard: {
      elevation: 2,
    },
  },
})
