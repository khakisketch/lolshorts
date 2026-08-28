let cachedVersion: string | null = null;

const FALLBACK_VERSION = "15.5.1";

export async function getDDragonVersion(): Promise<string> {
  if (cachedVersion) return cachedVersion;

  try {
    const res = await fetch(
      "https://ddragon.leagueoflegends.com/api/versions.json",
    );
    const versions: string[] = await res.json();
    cachedVersion = versions[0];
    return cachedVersion;
  } catch {
    return FALLBACK_VERSION;
  }
}

export function getChampionIconUrl(
  version: string,
  championName: string,
): string {
  return `https://ddragon.leagueoflegends.com/cdn/${version}/img/champion/${championName}.png`;
}

export function getChampionSplashUrl(championName: string): string {
  return `https://ddragon.leagueoflegends.com/cdn/img/champion/splash/${championName}_0.jpg`;
}
