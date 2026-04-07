<script setup lang="ts">
import DashboardLayout from '@/layouts/DashboardLayout.vue'

const rows = [
  {
    id: 1,
    header: 'Auth rollout checklist',
    type: 'Foundation',
    status: 'Done',
    target: 'A1',
    limit: 'Low',
    reviewer: 'Eddie Lake',
  },
  {
    id: 2,
    header: 'CSRF contract review',
    type: 'Security',
    status: 'In Progress',
    target: 'A2',
    limit: 'High',
    reviewer: 'Assign reviewer',
  },
  {
    id: 3,
    header: 'Captcha flow scaffold',
    type: 'Access',
    status: 'In Progress',
    target: 'A3',
    limit: 'Medium',
    reviewer: 'Assign reviewer',
  },
  {
    id: 4,
    header: 'Store token parity audit',
    type: 'Bridge',
    status: 'Done',
    target: 'B1',
    limit: 'High',
    reviewer: 'Jamik Tashpulatov',
  },
  {
    id: 5,
    header: 'Ownership scope checks',
    type: 'Backend',
    status: 'Done',
    target: 'Policy',
    limit: 'High',
    reviewer: 'Eddie Lake',
  },
  {
    id: 6,
    header: 'Safe public error catalog',
    type: 'Contracts',
    status: 'In Progress',
    target: 'Errors',
    limit: 'Medium',
    reviewer: 'Assign reviewer',
  },
]
</script>

<template>
  <DashboardLayout :rows="rows" />
</template>
