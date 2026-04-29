import { useCallback, useEffect, useState } from "react";
import { api, AiClient } from "../lib/tauri";

export function useClients() {
  const [clients, setClients] = useState<AiClient[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.detectClients();
      setClients(data);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { clients, loading, refresh };
}
