export interface PortInfo {
  private_port: number;
  public_port: number | null;
  port_type: string;
}

export interface ContainerInfo {
  id: string;
  names: string[];
  image: string;
  state: string;
  status: string;
  ports: PortInfo[];
  created: number;
  labels: Record<string, string>;
}

export interface ContainerGroup {
  name: string;
  containers: ContainerInfo[];
}

export interface ImageInfo {
  id: string;
  repo_tags: string[];
  size: number;
  created: number;
}

export interface VolumeInfo {
  name: string;
  driver: string;
  mountpoint: string;
  labels: Record<string, string>;
}

export interface NetworkInfo {
  id: string;
  name: string;
  driver: string;
  scope: string;
  containers: number;
}

export type Tab = "containers" | "images" | "volumes" | "networks";
