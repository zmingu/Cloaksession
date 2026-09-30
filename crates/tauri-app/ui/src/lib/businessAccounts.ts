import { invoke } from "@tauri-apps/api/core";

export type BusinessAccountKind =
  | "kuaishou-shop"
  | "kuaishou-live"
  | "kuaishou-mate"
  | "kuaishou-sub"
  | "jinniu";

export interface BusinessAccount {
  id: string;
  kind: BusinessAccountKind;
  displayName: string;
  platformUserId: string | null;
  profileId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface BusinessProfileState {
  account: BusinessAccount | null;
  scope: "jinniu" | "kuaishou" | null;
}

export interface SaveBusinessAccountInput {
  id?: string | null;
  profileId: string;
  kind: BusinessAccountKind;
  displayName: string;
  platformUserId: string | null;
}

/** Manual registration only. No login, cookie transfer or platform requests. */
export const businessAccounts = {
  list: (): Promise<BusinessAccount[]> =>
    invoke<BusinessAccount[]>("business_accounts_list"),
  profileState: (profileId: string): Promise<BusinessProfileState> =>
    invoke<BusinessProfileState>("business_accounts_profile_state", { profileId }),
  save: (input: SaveBusinessAccountInput): Promise<BusinessAccount> =>
    invoke<BusinessAccount>("business_accounts_save", { input }),
  unbind: (id: string): Promise<void> =>
    invoke<void>("business_accounts_unbind", { id }),
};
