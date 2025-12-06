import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useCurrentUser } from '../hooks/useAuth'
import LoginForm from './LoginForm'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
})

interface AuthProviderProps {
  children: React.ReactNode
}

function AuthGuard({ children }: AuthProviderProps) {
  const { data: user, isLoading } = useCurrentUser()

  if (isLoading) {
    return <div>Loading...</div>
  }

  if (!user) {
    return <LoginForm />
  }

  return <>{children}</>
}

export default function AuthProvider({ children }: AuthProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthGuard>{children}</AuthGuard>
    </QueryClientProvider>
  )
}
