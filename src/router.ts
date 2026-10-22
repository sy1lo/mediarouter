import { createRouter, createWebHashHistory } from 'vue-router'
import LibraryView from './views/LibraryView.vue'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'library', component: LibraryView },
    {
      path: '/rules',
      name: 'rules',
      component: () => import('./views/RulesView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('./views/SettingsView.vue'),
    },
  ],
})

export default router
