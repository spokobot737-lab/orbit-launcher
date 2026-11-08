import React, { useState, useEffect, useRef } from "react";
import { Layers, Monitor, Grid2X2 } from "lucide-react";
import { imageUrl } from "./api";
export function Cover({
  src,
  kind,
  name,
  className = "",
}: {
  src?: string;
  kind?: string;
  name?: string;
  className?: string;
}) {
  const [url, setUrl] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setUrl("");
    if (!src) return;
    let live = true;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((x) => x.isIntersecting)) {
          observer.disconnect();
          imageUrl(src)
            .then((u) => {
              if (live) setUrl(u);
            })
            .catch(() => {});
        }
      },
      { rootMargin: "150px" },
    );
    if (ref.current) observer.observe(ref.current);
    return () => {
      live = false;
      observer.disconnect();
    };
  }, [src]);
  return (
    <div ref={ref} className={`cover ${kind || ""} ${className}`}>
      {url ? (
        <img src={url} alt={name || ""} loading="lazy" />
      ) : kind === "minecraft" ? (
        <div className="voxel">
          <i />
          <i />
          <i />
          <i />
          <i />
          <i />
        </div>
      ) : kind === "roblox" ? (
        <Grid2X2 size={36} />
      ) : (
        <Monitor size={32} />
      )}
    </div>
  );
}
export function Empty({
  icon: Icon = Layers,
  title,
  children,
  action,
}: {
  icon?: typeof Layers;
  title: string;
  children?: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty-icon">
        <Icon size={27} />
      </span>
      <h3>{title}</h3>
      <p>{children}</p>
      {action}
    </div>
  );
}
export function Heading({
  eyebrow,
  title,
  subtitle,
  action,
}: {
  eyebrow: string;
  title: string;
  subtitle: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1>{title}</h1>
        <p>{subtitle}</p>
      </div>
      {action}
    </div>
  );
}
