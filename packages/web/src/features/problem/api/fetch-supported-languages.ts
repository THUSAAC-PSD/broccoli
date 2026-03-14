import type { ApiClient } from '@broccoli/web-sdk/api';

export interface SupportedLanguage {
  id: string;
  name: string;
  template: string;
  defaultFilename: string;
}

const TEMPLATE_BY_LANGUAGE_ID: Record<string, string> = {
  cpp: `#include <iostream>
using namespace std;

int main() {
    // Your code here
    return 0;
}`,
  c: `#include <stdio.h>

int main() {
    // Your code here
    return 0;
}`,
  java: `public class Main {
    public static void main(String[] args) {
        // Your code here
    }
}`,
  python3: `# Your code here
`,
  javascript: `// Your code here
`,
  rust: `fn main() {
    // Your code here
}
`,
  go: `package main

import "fmt"

func main() {
    fmt.Println("Your code here")
}
`,
  typescript: `// Your code here
`,
};

export async function fetchSupportedLanguages(
  apiClient: ApiClient,
): Promise<SupportedLanguage[]> {
  const { data, error } = await apiClient.GET('/plugins/registries');
  if (error) throw error;

  return (data.languages ?? []).map((language) => ({
    id: language.id,
    name: language.name,
    defaultFilename: language.default_filename,
    template: TEMPLATE_BY_LANGUAGE_ID[language.id] ?? '',
  }));
}
