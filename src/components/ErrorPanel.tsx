interface ErrorPanelProps {
  message: string;
}

export function ErrorPanel({ message }: ErrorPanelProps) {
  return (
    <div role="alert" className="error-panel">
      {message}
    </div>
  );
}
