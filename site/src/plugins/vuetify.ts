import 'vuetify/styles'
import '@mdi/font/css/materialdesignicons.css'
import { createVuetify } from 'vuetify'
import * as components from 'vuetify/components'
import * as directives from 'vuetify/directives'

export default createVuetify({
  components,
  directives,
  theme: {
    defaultTheme: 'light',
    themes: {
      light: {
        dark: false,
        colors: {
          // Primary - Material Blue 600
          primary: '#1E88E5',
          // Secondary - Blue Grey 700
          secondary: '#455A64',
          // Accent - Light Blue 500
          accent: '#03A9F4',
          // Error - Material Red 600
          error: '#E53935',
          // Info - Material Blue 500
          info: '#2196F3',
          // Success - Material Green 600
          success: '#43A047',
          // Warning - Material Orange 700
          warning: '#FB8C00',
          // Background - White
          background: '#FFFFFF',
          // Surface - Grey 50
          surface: '#FAFAFA',
          // On colors
          'on-primary': '#FFFFFF',
          'on-secondary': '#FFFFFF',
          'on-surface': '#212121',
          'on-background': '#212121',
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
