import axios from 'axios'

const api = axios.create({
  baseURL: '/backoffice/api',
  withCredentials: true,
  xsrfCookieName: 'XSRF-TOKEN',
  xsrfHeaderName: 'X-XSRF-TOKEN',
})

export default api
