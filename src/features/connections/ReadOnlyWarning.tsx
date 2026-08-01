export function ReadOnlyWarning() {
  return (
    <div role="note" className="warning-banner">
      <strong>Use uma credencial somente leitura.</strong> O queryboard nunca
      escreve no banco, mas essa garantia depende também da permissão do
      usuário informado aqui. Cadastre uma credencial com privilégio apenas
      de <code>SELECT</code> — nunca a mesma credencial de aplicação com
      permissão de escrita.
    </div>
  );
}
