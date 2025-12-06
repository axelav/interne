import { useState } from "react";
import { Form, Input, Button } from "./Forms";
import { useLogin, useRegister } from "../hooks/useAuth";
import styles from "../styles/Forms.module.css";

export default function LoginForm() {
  const [isRegister, setIsRegister] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  const login = useLogin();
  const register = useRegister();

  const handleSubmit = async () => {
    if (!email || !password) {
      setError("Email and password are required.");
      return;
    }

    setError("");

    try {
      if (isRegister) {
        await register.mutateAsync({ email, password });
      } else {
        await login.mutateAsync({ email, password });
      }
    } catch (err: any) {
      setError(err.message || "Authentication failed");
    }
  };

  return (
    <div className={styles.authContainer}>
      <h2>{isRegister ? "Register" : "Login"}</h2>
      <Form>
        {error && <div className={styles.error}>{error}</div>}
        <Input type="email" value={email} label="Email" onChange={setEmail} />
        <Input
          type="password"
          value={password}
          label="Password"
          onChange={setPassword}
        />
        <Button onClick={handleSubmit}>
          {isRegister ? "Register" : "Login"}
        </Button>
      </Form>
      <button
        className={styles.toggleAuth}
        onClick={() => {
          setIsRegister(!isRegister);
          setError("");
        }}
      >
        {isRegister
          ? "Already have an account? Login"
          : "Don't have an account? Register"}
      </button>
    </div>
  );
}
