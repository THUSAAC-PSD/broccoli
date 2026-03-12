import type { ApiClient } from '@broccoli/web-sdk/api';

export interface SupportedLanguage {
  id: string;
  name: string;
  template: string;
  defaultFilename: string;
}

/**
 * Mock API: fetch supported programming languages.
 * Replace with a real endpoint call when backend is ready.
 */
export async function fetchSupportedLanguages(
  _apiClient: ApiClient,
): Promise<SupportedLanguage[]> {
  return [
    {
      id: 'cpp',
      name: 'C++',
      defaultFilename: 'solution.cpp',
      template: `#include <iostream>
using namespace std;

int main() {
    // Your code here
    return 0;
}`,
    },
    {
      id: 'python',
      name: 'Python',
      defaultFilename: 'solution.py',
      template: `# Your code here
`,
    },
    {
      id: 'java',
      name: 'Java',
      defaultFilename: 'Main.java',
      template: `public class Main {
    public static void main(String[] args) {
        // Your code here
    }
}`,
    },
    {
      id: 'c',
      name: 'C',
      defaultFilename: 'solution.c',
      template: `#include <stdio.h>

int main() {
    // Your code here
    return 0;
}`,
    },
  ];
}
